# Phase 9.2B — Stage A, Batch 3

**Fix `agent_execution_tests::setup_harness` SQLite fixture cleanup**

**Date:** 2026-10-10
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind) — unchanged, nothing committed
**Verdict:** **PASS**
**Scope delivered:** 1 fixture helper; **15 call sites**; 7 new lifecycle tests
**Production code changed:** none

---

## 0. Scope statement

Phase 9.2 remains **open**. One helper was fixed. Seven further helpers still leak, now
**spot-measured at +157 directories per combined run** (§13).

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
| Modified / Untracked / Deleted / Staged | 25 / 29 / **0** / 0 |
| `.tmp*` artifacts | **9,375** |
| `C:` free | **46.11 GB** |

### 1.1 Out-of-scope files pinned by SHA before any edit

| file | SHA-256 (first 16) | status at end of batch |
|---|---|---|
| `src/store.rs` | `97E79B8F876D7219` | **UNCHANGED** |
| `src/integrity.rs` | `EAEA60CFA367BCB6` | **UNCHANGED** |
| `src/integrity_tests.rs` | `CC1205A7FBB688A6` | **UNCHANGED** |
| `src/phase_8_quality_tests.rs` | `B5D3CA6E616187F1` | **UNCHANGED** |
| `src/phase_9_readiness_tests.rs` | `99D402A29E57EDCF` | **UNCHANGED** |
| `src/exploration_tests.rs` | `8A8980B655B0C2D5` | **UNCHANGED** |

### 1.2 The target file was clean at HEAD

`agent_execution_tests.rs` is **tracked**, and `git diff --numstat` against HEAD returned **empty**
before this batch — i.e. it carried **no pre-existing uncommitted work**. That made `git diff` an
authoritative measure for this batch, which is why §11 can state the change as exactly 15 insertions
and 0 deletions.

---

## 2. Actual helper and call-site count

| | value |
|---|---:|
| `setup_harness()` call sites | **15** |
| tests using `setup_harness()` | **15** (1:1 — no test creates two fixtures) |
| binding shape A | `let (store, registry, evidence, _dir) = setup_harness().await` ×14 |
| binding shape B | `let (_store, _registry, evidence, _dir) = setup_harness().await` ×1 |
| `pool.close()` before this batch | **0** |
| `impl Drop` in file | **0** |
| tests in file | 33 (before this batch's 7 additions) |

The historical count of 15 was **accurate**.

### 2.1 Baseline measurement (taken before any edit)

```
cargo test agent_execution_tests -> ok. 33 passed; 0 failed; 0 ignored
.tmp delta = 15
```

---

## 3. Verified ownership and teardown paths

This helper has a **more complex ownership graph** than any previous batch, so it was mapped
explicitly rather than assumed.

### 3.1 The fixture returns a 4-tuple

```rust
async fn setup_harness() -> (Store, Arc<ExecutorRegistry>, Arc<LocalEvidenceStore>, tempfile::TempDir)
```

A shape not seen in Phases 9.2A or Batch 1/2. `Arc<ExecutorRegistry>` is new.

| returned object | holds the SQLite pool? | evidence |
|---|---|---|
| `Store` | **yes** | `pub struct Store { pub pool: SqlitePool }` |
| `Arc<ExecutorRegistry>` | **no** | fields are `by_kind: HashMap<String, Arc<dyn Executor>>` and `default: Arc<dyn Executor>` |
| `Arc<LocalEvidenceStore>` | **no** | `pub struct LocalEvidenceStore { root: PathBuf }` |
| `TempDir` | n/a | owns the directory containing the database |

Every executor type in the registry was also checked — `SimulatedExecutor`, `CompiledApiExecutor`,
`PlaywrightExecutor`, `NewmanExecutor`, `PytestExecutor`, `MaestroExecutor`, `SchemathesisExecutor` —
**none holds a `Pool` or `Store`**.

### 3.2 `store` is moved out of the test's hands

14 of 15 sites immediately do:

```rust
let dispatcher = AgentExecutionDispatcher::new(store, policy, registry, evidence, None);
```

`new()` takes `store: Store` **by value**. Two sites pass `store.clone()` instead. The remaining
site binds `_store` and never builds a dispatcher.

`AgentExecutionDispatcher` declares `pub store: Store`, so the pool remains reachable after the move:
`dispatcher.store.pool.close().await`.

### 3.3 **The complication unique to this suite — a detached task holds the pool**

This is the finding that mattered most, and Batch 2's "no spawned tasks" result could not be assumed.

`agent_execution_tests.rs` itself contains **zero** concurrency primitives (`tokio::spawn` 0,
`spawn_blocking` 0, `JoinHandle` 0, `tokio::join` 0). But the production code it calls does:

```rust
// agent_execution.rs:403 — AgentExecutionDispatcher::dispatch
engine::spawn_run(
    record.clone(),
    self.registry.clone(),
    run_plan.concurrency,
    self.store.clone(),        // <-- a Store clone goes into a detached task
    self.evidence.clone(),
    ...
);
```

and `agent_execution.rs:555` (`approve`) does the same through `engine::approve_and_spawn`.

`engine::spawn_run` is **fire-and-forget**:

```rust
pub fn spawn_run(...) {
    tokio::spawn(async move {
        mark_run_running(&record, &store).await;
        ...
    });
}   // returns (); no JoinHandle is returned or awaited
```

`agent_execution.rs` itself contains **zero** spawns, so the dispatcher adds no further background
work. But the run it starts is detached and outlives the call.

### 3.4 Do the tests wait for those runs?

**No.** Every test asserts on the dispatch/approval *response* and returns:

| test | reaches `spawn_run`? | waits for run completion? |
|---|---|---|
| `test_17_viewer_role_cannot_dispatch_execution` | no — `Unauthorized` before the gate | n/a |
| `test_21_policy_blocked_executor_denied` | no — `PolicyDenied` before the gate | n/a |
| `test_19_admin_key_can_dispatch_any_project` | **yes** | **no** — asserts `res.is_ok()` then returns |
| `test_20_allowed_plan_auto_spawns_and_executes` | **yes** | **no** — asserts the response then returns |
| `test_24_operator_can_approve_held_execution` | **yes** (via `approve_and_spawn`) | **no** |
| `test_30_observation_...` | **yes** | reads observation only, does not await the run |
| `test_31_idempotency_...` | **yes** | **no** |

No test in the file calls `tokio::time::sleep` or any completion poll.

### 3.5 Consequence, and how it was resolved

At teardown time a **detached task may still hold a `Store` clone**. This is precisely the hazard
Step 2 warns about, so it was resolved by measurement rather than reasoning:

* **`Pool::close()` completes successfully even with a detached clone in flight** — measured (§6,
  §8: 15/15 leaks eliminated, and the lifecycle test
  `p92b3_detached_run_does_not_block_cleanup` passes 10 consecutive runs).
* The close releases the file handles for the **shared** pool, so the detached clone no longer holds
  the SQLite file open and the `TempDir` is removable.
* **Residual coupling, stated plainly:** after the close, the detached run's *remaining* database
  writes will fail, because a closed `Pool` will not open new connections. No test asserts on those
  writes (3.4), and 10 consecutive runs produced **zero panics**, so the failure is swallowed by the
  detached task as it already was before this change. This is a real consequence of the fix, not an
  idealised claim.

No production code was changed to coordinate this, and none was needed.

---

## 4. Root cause and fix

Unchanged from earlier phases and re-confirmed at the pinned dependency:

1. `Store::connect` sets **WAL**, so `-wal`/`-shm` sidecars stay open for the life of the connection.
2. On Windows `remove_dir_all` **fails** while those handles are open.
3. `tempfile` **swallows** that error, so the directory leaks silently.

```rust
// sqlx-core-0.9.0, src/pool/mod.rs:441
pub fn close(&self) -> impl Future<Output = ()> + '_
```

`close()` returns a `Future` that must be awaited; an ordinary `Drop` would drop it unpolled and
release nothing.

### 4.1 The fix

```rust
    dispatcher.store.pool.close().await;   // 14 sites
    _store.pool.close().await;             //  1 site (test that never builds a dispatcher)
```

placed as the **last statement** of each test body. Both forms are the identical mechanism; the
difference is only which binding still owns the `Store` after the dispatcher is constructed.

For the `_store` site, the binding was deliberately **left unchanged** — using an underscore-prefixed
binding is legal and avoids touching an existing line.

---

## 5. Files changed

| file | change |
|---|---|
| `services/orchestrator/src/agent_execution_tests.rs` | **+15 / −0** in the existing tests; plus `setup_harness_in()` helper and 7 lifecycle tests appended |
| `docs/next-intelligence/PHASE_9_2B_BATCH_3_REPORT.md` | this report (new) |

**Nothing else.** All six out-of-scope files are SHA-verified unchanged.

---

## 6. Results of all 10 repeated runs

| run | exit code | passed / failed / ignored | new `.tmp` | panics | notes |
|---:|---:|---|---:|---:|---|
| 1 | 0 | 40 / 0 / 0 | **0** | 0 | clean |
| 2 | 0 | 40 / 0 / 0 | **0** | 0 | clean |
| 3 | 0 | 40 / 0 / 0 | **0** | 0 | clean |
| 4 | 0 | 40 / 0 / 0 | **0** | 0 | clean |
| 5 | 0 | 40 / 0 / 0 | **0** | 0 | clean |
| 6 | 0 | 40 / 0 / 0 | **0** | 0 | clean |
| 7 | 0 | 40 / 0 / 0 | **0** | 0 | clean |
| 8 | 0 | 40 / 0 / 0 | **0** | 0 | clean |
| 9 | 0 | 40 / 0 / 0 | **0** | 0 | clean |
| 10 | 0 | 40 / 0 / 0 | **0** | 0 | clean |

**10/10 exit 0, 10/10 zero new artifacts, zero panics.** Baseline was **+15**.

Panics were counted explicitly because this suite detaches real runs — zero confirms the detached
tasks did not surface errors during teardown.

Failure detection used `-cmatch 'test result: FAILED'` (case-sensitive).

---

## 7. Per-run artifact measurements

Per-run values are the `.tmp` delta across `%TEMP%` for the whole `agent_execution_tests` suite, all
of which uses `setup_harness()`:

```
run  1: 0        run  6: 0
run  2: 0        run  7: 0
run  3: 0        run  8: 0
run  4: 0        run  9: 0
run  5: 0        run 10: 0
```

Every test in this suite uses the one helper, so the suite-level delta is attributable to it with no
ambiguity. The lifecycle tests additionally assert against their own private roots
(`%TEMP%\aiqa_p92b3_artifacts\<test_name>\`), so a green global count could not hide a failure.

### 7.1 Repository totals

| point | `.tmp*` dirs | `C:` free |
|---|---:|---:|
| Batch start | 9,375 | 46.11 GB |
| Batch end | **9,405** | **46.00 GB** |

The +30 is from the control suites run for comparison in §13, not from `agent_execution_tests`.

---

## 8. Mutation results and proof the mutation applied

### 8.1 The first mutation attempt was invalid and was discarded

The first attempt replaced each close with `let _ = &store;`. It **did not compile**:

```
error[E0382]: borrow of moved value: `store`: value borrowed here after move
  --> src/agent_execution_tests.rs:449  (and 11 more)
error: could not compile `aiqa-orchestrator` (lib test) due to 12 previous errors
```

This is expected and is a direct consequence of §3.2: in 12 of the 14 dispatcher tests `store` was
**moved** into the dispatcher, so it cannot be borrowed afterwards.

The mutation run reported `delta 0` and **that number was meaningless** — no binary was produced, so
no test ran. Two independent signals caught it: the binary's mtime was **not** newer than the
mutated source, and the output contained no `test result:` line at all. Both are now mandatory checks
in this batch's procedure.

### 8.2 The valid mutation: pure deletion

Because the close statements are self-contained lines, **deleting** them compiles everywhere and
reproduces the original pre-fix state for those tests.

| proof | value |
|---|---|
| pre-mutation SHA-256 (recorded to sidecar **before** mutating) | `225ca7860df3376e45c8443a160d48c9803c1b9f16faaa2fb9a10e34993242e8` |
| mutated SHA-256 | `bffbccc81fd0972f7890c1018c41b63694bfba2c7e70a7dcdb45ac43c9debcd3` |
| sha changed | **True** |
| lines deleted | **15** (14 `dispatcher.store...` + 1 `_store...`) |
| residual closes in the pre-existing region | **0** |
| lifecycle-test closes preserved | **7** |
| compile errors | **0** |
| `Compiling aiqa-orchestrator v0.1.0` emitted | **yes** |
| binary mtime 22:44:47 > mutated source mtime 22:44:22 | **True** |

### 8.3 Result

```
cargo test agent_execution_tests -> ok. 40 passed; 0 failed; 0 ignored
.tmp delta = 15
```

**The leak returned at exactly the pre-edit baseline of 15** — and, again, **the suite stayed
completely green.** A passing exit code is not evidence that cleanup succeeded.

### 8.4 Restoration

| check | result |
|---|---|
| Expected SHA-256 (pre-mutation sidecar) | `225ca786…93242e8` |
| Restored SHA-256 | `225ca786…93242e8` |
| sha match | **True** |
| Byte-identical to pre-mutation backup | **True** |
| Awaited closes present | **22** (15 existing tests + 7 lifecycle tests) |
| Mutation residue | **0** |
| Backup and `.sha` sidecar deleted | **True** |
| Suite after restore | `ok. 40 passed`, **delta 0** |

No mutated source or binary remains.

---

## 9. Lifecycle tests added (7)

| test | Properties covered |
|---|---|
| `p92b3_fixture_is_usable_and_fully_migrated` | **A, B** — 5 core tables migrated; a real project write round-trips; the evidence directory is real |
| `p92b3_agent_execution_behaviour_is_unchanged` | **C, J** — a permitted plan still dispatches to `Executing` with `tasks_count == 2` |
| `p92b3_detached_run_does_not_block_cleanup` | **D, E, I** — **the test that matters most here.** Dispatches successfully (asserting `Executing`, so the detached run really was spawned), then closes the pool and asserts the directory is gone |
| `p92b3_close_removes_the_directory_despite_wal_and_shm` | **E, F** — asserts WAL/SHM sidecars **exist while live**, then the directory is **gone** |
| `p92b3_repeated_fixtures_accumulate_nothing` | **G** — 10 cycles leave **0** directories |
| `p92b3_parallel_fixtures_remain_isolated` | **H** — 8 spawned tasks; each asserts its database existed first, then removes exactly its own |
| `p92b3_teardown_failure_is_observable` | **K** — uses fallible `TempDir::close()` and asserts `is_ok()` |

**Property J** ("existing execution, telemetry and failure assertions remain intact") is
additionally evidenced structurally: all **33** original tests still exist and pass, with **0**
lines removed from the file (§11).

### 9.1 Isolated measurement root

```rust
fn p92b3_root(test_name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join("aiqa_p92b3_artifacts").join(test_name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create isolated artifact root");
    root
}
```

Fixtures are built by a new `setup_harness_in(&root)` that uses `tempfile::tempdir_in`.

**`setup_harness()` itself was deliberately left untouched** — it still calls `tempfile::tempdir()`,
so the 15 existing call sites keep their exact directory location and naming. No behavioural
equivalence needed to be argued for.

**Global `.tmp*` is never the sole assertion.** Test parallelism was **not** disabled; the parallel
test spawns 8 tasks and awaits them together.

---

## 10. Regression results

| suite | result | `.tmp` delta |
|---|---|---:|
| `agent_execution_tests` | ok. **40 passed; 0 failed; 0 ignored** | **0** |
| `agent_tests` | ok. **20 passed; 0 failed** | **0** |
| `exploration_tests` (Batch 2) | ok. **35 passed; 0 failed; 1 ignored** | **0** |
| `phase_8_quality_tests` (Batch 1) | ok. **20 passed; 0 failed** | **0** |
| `phase_9_readiness_tests` (Batch 1) | ok. **21 passed; 0 failed** | **0** |
| `phase_9_2a_lifecycle_tests` (9.2A) | ok. **6 passed; 0 failed** | **0** |
| `integrity_tests` (9.2A) | ok. **34 passed; 0 failed; 1 ignored** | **0** |

**No full backend suite was run**, by instruction; that check is deferred until the staged migration
completes.

`setup_harness_in` is private to this file, so **no shared test helper changed** and the Phase 8/9
runs above are canaries rather than requirements.

---

## 11. Diff verification

Because the file was clean at HEAD, `git diff` is authoritative.

```
15   0   services/orchestrator/src/agent_execution_tests.rs
```

| check | result |
|---|---|
| lines added in the existing tests | **15** |
| lines added by this batch overall | 15 teardown statements + `setup_harness_in` + 7 tests |
| **lines removed** | **0** |
| distinct added content in the test bodies | `+ dispatcher.store.pool.close().await;` ×14<br>`+ _store.pool.close().await;` ×1 |
| existing tests removed or weakened | **none** — 33 → 40, all originals pass |
| early `return` in file | 0 |
| `?` operator lines | 0 |

### 11.1 Formatting was contained, and proven so

`cargo fmt --check` initially reported **7 diffs** in this file, all inside the appended block
(lines ≥ 1047). No write-mode `rustfmt` was run against the file. A copy was formatted instead, and
the changed-line ranges (`1047, 1132–1138, 1158, 1172–1178, 1207, 1233, 1246`) were inspected before
writing back.

The decisive guarantee is stronger than a range check: after writing back, a **multiset comparison**
confirmed that **0 pre-edit lines were lost or altered**.

`cargo fmt --check` is now clean except `auth.rs:317`.

---

## 12. Production code untouched, artifacts preserved

| check | result |
|---|---|
| `store.rs`, `integrity.rs`, `integrity_tests.rs` | SHA **unchanged** |
| `phase_8_quality_tests.rs`, `phase_9_readiness_tests.rs`, `exploration_tests.rs` | SHA **unchanged** |
| Only `agent_execution_tests.rs` written this batch | **yes** |
| Migrations or routes added | **none** |
| `impl Drop` introduced as async shutdown | **none** |
| Blocking the async runtime | **none** |
| Global cleanup script | **none** |
| Commits / pushes | **none**; HEAD still `7930507` |
| Repo state | 25 → **26 modified** (the target file) / 29 → 30 untracked (+1 report) / **0 deleted** / 0 staged |

### 12.1 Historical artifacts preserved

No `Remove-Item`, `rmdir` or sweep was issued against any historical `.tmp*` directory; counts only
ever increased. The only paths removed are
`%TEMP%\aiqa_p92b3_artifacts\<test_name>\`, namespaces created by this batch's own tests.

---

## 13. Remaining leaking helpers

All re-measured at the end of this batch.

| suite | helper | sites | `.tmp` delta | status |
|---|---|---:|---:|---|
| `integrity_tests` | `harness` | 29 | **0** | fixed 9.2A |
| `phase_8_quality_tests` | `Db::new` | 15 | **0** | fixed Batch 1 |
| `phase_9_readiness_tests` | `Db::new` | 16 | **0** | fixed Batch 1 |
| `exploration_tests` | `harness` | 19 | **0** | fixed Batch 2 |
| `agent_execution_tests` | `setup_harness` | 15 | **0** | **fixed this batch** |
| `governed_healing_tests` | `setup_healing_harness` | 42 | **+43** | untouched |
| `engine_tests` | `temp_store` (4 of 32 already fixed) | 32 | **+31** | untouched |
| `autopilot_store` | `temp_store` | 20 | **+21** | untouched |
| `qa_memory_tests` | `temp_store` | 23 | **+25** | untouched |
| `qa_memory_retrieval_tests` | `temp_store` | 20 | **+21** | untouched |
| `trace_tests` | `temp_store` | 18 | **+18** | untouched |
| `change_impact_tests` | `temp_store` | 16 | **+14** | untouched |
| `retention_and_metrics_tests` | `setup` | 9 | **+10** | untouched |
| `api_tests` | `test_state` / `test_state_with_compiled_api` | 116 / 66 | not measured | untouched |

**Spot-measured per suite, not a repository total.** The remaining measured helpers total **+157**.

### 13.1 Production API blockers

**None encountered.** No helper was silently deferred.

`api_tests` remains the only unproven case: `test_state` returns an `AppState`, not a bare `Store`,
so its ownership graph must be analysed from scratch — as §3 was for this batch — rather than assumed
from the simpler fixtures.

---

## 14. Verdict

# Phase 9.2B Batch 3: **PASS**

| claim | evidence |
|---|---|
| 15 call sites fixed (estimate was accurate) | counted from source; `git diff` = 15/0 |
| Leak eliminated | **+15 → 0**, 10/10 runs, plus 7 regression suites |
| Complex ownership handled | §3 — 4-tuple, moved `Store`, `Arc<ExecutorRegistry>`, detached run task |
| Detached task does not defeat the fix | `p92b3_detached_run_does_not_block_cleanup` passes; 10 runs with 0 panics |
| Residual coupling disclosed | §3.5 — post-close writes by the detached run will fail; no test asserts on them |
| Mutation actually applied | sha changed, 15 deletions, 0 residual, `Compiling` emitted, binary newer than source |
| Invalid mutation discarded | §8.1 — compile error, stale binary, no `test result:` line; **not** counted as evidence |
| Mutation proves the fix is load-bearing | leak returns at exactly **15** — while the suite stays **green** |
| Restoration exact | sha match, byte-identical, 0 residue, backups deleted |
| Production code untouched | 6 SHA comparisons, all unchanged |
| No formatting blow-up | 7 diffs, all in new code; 0 pre-edit lines lost |
| No artifact deleted | no sweep; counts only increased |
| No regressions | 7 suites green, 176 tests |

**Qualification:** 1 helper of ~39; 15 call sites of ~700. **Phase 9.2 remains open.**

---

## 15. Recommended next step

**Recommended: `governed_healing_tests::setup_healing_harness` — 42 sites, measured +43.**

Rationale:

* It is now the **largest single-file leak** and the largest single measured delta in the repository.
* One file, one helper, one ownership model — the same shape as this batch, so the 10-run +
  mutation + restore loop applies unchanged.
* It removes **27%** of the remaining measured leakage (+43 of +157) in a single batch.
* `engine.rs` drives the healing path and **is** the module containing the fire-and-forget
  `spawn_run`, so the detached-task analysis in §3 must be repeated. That analysis is now a known,
  cheap procedure rather than an unknown.

Suggested Batch 4 scope, in order:

1. `governed_healing_tests::setup_healing_harness` — 42 sites, +43.
2. `engine_tests::temp_store` — 32 sites, +31 (28 of 32 sites still leak; 4 already close).
3. `qa_memory_tests::temp_store` — 23 sites, +25.

Hold `api_tests.rs` (116 + 66 sites) for its **own** batch. It is the largest and the only family
whose return type (`AppState`) makes the §3 ownership analysis non-trivial.

### 15.1 Process rules for Batch 4

Carried forward, each one earned:

* **Never run write-mode `rustfmt` on a whole test file.** Format a copy; prove containment with a
  **multiset check that no pre-edit line was lost or altered** (§11.1) — a range check alone has
  already produced one false pass in this work.
* **Unique backup name plus a `.sha` sidecar recorded before mutating.**
* **A mutation must compile.** Replace-with-a-statement fails whenever a binding was moved into a
  struct; deleting a self-contained statement is the safer default.
* **Verify the binary is newer than the mutated source, and that a `test result:` line exists**, before
  reading any delta as evidence (§8.1).
* **Do not trust a green exit code.** §8.3: the suite passed 40/40 with the entire fix removed.
* **Expect N−1** when a suite contains a test that never opens a handle.
* **Map the ownership graph explicitly**, including production code the fixture calls — this batch's
  detached `spawn_run` was invisible from the test file and only appeared by reading
  `agent_execution.rs`.

---

## 16. Unchanged open items

* **D3** — two `failure_severity` implementations; **not** claimed equivalent.
* **D5** — the 10 ms duration floor is provisional; empirical inter-write timing unmeasured.
* **D6** — `set_task_status` enum redesign deferred.
* **Staleness criterion** for readiness test #6 — undefined in the roadmap.
* **Roadmap deliverables still owed:** `PHASE_7_PASS.md`, `PHASE_8_PASS.md`, `PHASE_9_PASS.md`,
  `PHASE_10_QA_COPILOT_REPORT.md`.
* **Phase 9 qualification:** still recommended as `PASS WITH DEFERRED ACCEPTANCE CRITERIA` (6 of 13
  roadmap-required tests unimplemented).
* **Production `Store::close()`** remains **unauthorized** and unimplemented. Measured cost so far:
  **5 helpers, 99 call sites, 0 production changes**.
* **Four pre-existing clippy lints** in `agent_execution_tests.rs`
  (`assert_eq!(_, false)` at lines 530, 594, 624, 668) were confirmed present before this batch and
  left unchanged, since altering existing assertions is outside the fixture-cleanup scope.

**Batch 4 was not started. Phase 10 was not started.**