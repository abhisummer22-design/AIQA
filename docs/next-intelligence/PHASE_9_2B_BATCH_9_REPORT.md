# Phase 9.2B — Batch 9 Report

## Process-Lifetime, Key-Scope, WebSocket, RBAC and Run-Map-Retention Fixtures

**Date:** 2026-10-11
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind) — nothing committed, nothing pushed
**Verdict: PASS**

---

## 0. Headline numbers

| | |
|---|---:|
| Unique fixture helpers fixed | **5** |
| Call sites (source statements) | **41** (estimate was 35) |
| Runtime fixtures (stores actually built) | **42** |
| Tests owning a fixture | **41** of 51 |
| Measured leaks eliminated | **42** (estimate was 42) |
| Post-fix delta, all five suites | **0** |
| Fault-injection closes in scope | **0** — none existed (§4) |
| Repeated suite runs | **50** (10 per suite), all exit 0, all delta 0 |
| Individual per-test runs | **51**, all exit 0, all delta 0 |
| Files changed | 5 test files — **no production file** |

---

## 1. Inventory reconciliation (Step 1)

### 1.1 The `564` discrepancy — resolved

The Batch 8 report stated 564 outstanding, then cited 176 re-measured plus 308 `api_tests`.
**176 + 308 = 484, not 564.** The brief was right to challenge this. The 564 was *not* wrong — the
report simply failed to state a third component.

```
564  =  176  +  308  +  80
         │        │      └── families whose figures were carried forward unmeasured
         │        │          (risk_semantics, rbac_probe, pre_phase_8, phase_7_4,
         │        │           retention_and_metrics — none re-measured in Batch 8)
         │        └───────────── api_tests (Batch 14), previously measured at 308
         └────────────────────── 14 suites re-measured during Batch 8
```

Cross-checked against the inventory's own batch plan, the 564 decomposes exactly:

| planned batch | content | figure |
|---|---|---:|
| Batch 9 | this batch | 42 |
| Batch 10 | `trace_chain_tests`, `risk_signal_tests`, `risk_semantics_tests`, `rbac_probe_tests` | 30 |
| Batch 11 | `phase_7_1`, `phase_7_3`, `pre_phase_8`, `phase_7_4` | 45 |
| Batch 12 | `integration_e2e`, `journey`, `operations`, `adaptive_intelligence`, `retention_and_metrics` | 59 |
| Batch 13 | `autopilot_steps/generate/execute/report` + `autopilot_runner` | 80 |
| Batch 14 | `api_tests` | 308 |
| | **total** | **564** |

**After Batch 9: 564 − 42 = 522 outstanding.**

### 1.2 Batch 9 measured against the plan

The inventory predicted **42** artifacts for these five families. This batch measured **42**
pre-batch and **0** post-batch. The prediction was exact.

### 1.3 Distinct quantities, kept distinct

| quantity | value | note |
|---|---:|---|
| Unique fixture helpers in scope | 5 | one per file |
| **Call sites** (source statements) | **41** | what a text scan finds |
| **Runtime fixtures** (stores built) | **42** | one call site sits in a 2-iteration loop and runs twice |
| **Tests owning a fixture** | **41** of 51 | 10 tests own none |
| **Measured artifact delta** | **42** | directories per full run |

41 ≠ 42 because `key_scope_tests::a_project_scoped_admin_never_ends_up_with_a_global_key_however_it_asks`
builds its fixture *inside* a `for` loop over two cases. No completion percentage is derived from
these; they are not interchangeable.

---

## 2. Initial repository state

| item | value |
|---|---|
| Branch / HEAD | `main` / `7930507` |
| Upstream | 0 ahead / 0 behind |
| Modified / Untracked / Deleted / Staged | 30 / 37 / **0** / 0 |
| `.tmp*` directories | 2,938 |
| `C:` free | 39.06 GB |
| `store.rs` SHA (production, pinned) | `97E79B8F876D7219` — unchanged |

Pre-batch pins, backed up to `%TEMP%\B9_backup` before any edit:

| file | sha256 | lines |
|---|---|---:|
| `process_lifetime_maps_tests.rs` | `46F3523FC3C8644D` | 395 |
| `key_scope_tests.rs` | `0A03BB1C5813EB6D` | 411 |
| `ws_tests.rs` | `7BACA6D6FEC1ECDA` | 540 |
| `rbac_matrix_tests.rs` | `A7F6D84758D74148` | 498 |
| `run_map_retention_tests.rs` | `8E70ACA1E3F80CBB` | 288 |

All five are standalone `src/*.rs` test modules. **None is inline in a production file**, so the
Batch 8 `store::history_tenant_tests` blocker does not apply here.

---

## 3. Fixtures

All five use a different shape from Batches 7–8 — an **`AppState`**, not a bare `Store`:

```rust
async fn test_state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::connect(&dir.path().join("<name>.sqlite3")).await.expect("connect");
    let evidence = LocalEvidenceStore::new(dir.path().join("evidence")).expect("evidence");
    (AppState::new(registry, store, policy, evidence, ai_infra, embedder), dir)
}
```

DB names: `maps.sqlite3`, `keyscope.sqlite3`, `ws.sqlite3`, `rbac.sqlite3`, `retention.sqlite3`.
One of them (`run_map_retention_tests::the_idempotency_cache_is_bounded…`) builds its own bare `Store`
inline instead.

`AppState` carries 12 fields; `store: Store` is the only one holding the SQLite pool directly, and
`Store::clone()` shares a single pool — so **one awaited close releases every handle** regardless of
how many clones exist. That property is what makes a single close sufficient even where the state is
cloned into a live server.

---

## 4. Existing `pool.close()` classification (Step 3, before any edit)

| file | existing closes | classification |
|---|---:|---|
| `process_lifetime_maps_tests.rs` | **0** | — |
| `key_scope_tests.rs` | **0** | — |
| `ws_tests.rs` | **0** | — |
| `rbac_matrix_tests.rs` | **0** | — |
| `run_map_retention_tests.rs` | **0** | — |

**No existing closures of any kind.** There is nothing to classify, nothing to preserve, and
**Mutation 2 (fault-injection removal) is not applicable to this batch** — there is no
fault-injection close to remove. This is recorded as a limitation rather than reported as a pass:
the fault-injection behaviour those five families rely on is simply not present in them. The
change-impact family that does carry such closes had them preserved in Batch 8.

---

## 5. Ownership and background-work analysis

### 5.1 The four background-free families

`process_lifetime_maps_tests`, `key_scope_tests`, `rbac_matrix_tests` and `run_map_retention_tests`
contain **no** `tokio::spawn`, no `spawn_blocking`, no `JoinHandle`, no `thread::spawn`, no
`spawn_run`, no `axum::serve`. No handler is reachable that could start detached work. A plain
appended awaited close is correct for all 32 sites.

### 5.2 `ws_tests` — the real risk, and why the close is still safe

`ws_tests` does **not** use `tower::oneshot` like `api_tests`. It binds a real router to a real
ephemeral TCP port and drives it with a real WebSocket client:

```rust
async fn spawn_server(state: AppState) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let app = crate::api::router(state);
    tokio::spawn(async move {                       // <-- JoinHandle DISCARDED
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}
```

The spawned task owns the `AppState` (and therefore the pool) and is only torn down when
`#[tokio::test]`'s per-test runtime drops — *after* the test body returns. So at teardown a live
server still holds the state. Two hazards were reasoned about and then tested:

1. **Could the close block?** `Pool::close()` calls `mark_closed()` *before* awaiting
   `semaphore.acquire(max_connections)`. After the flag is set, any *new* `acquire()` fails
   immediately rather than queueing, so a task racing for a fresh connection cannot deadlock against
   the close. Only a connection already checked out can hold it, and at test end no handler is
   mid-query — every test either completed its request or is waiting on the socket.
2. **Could the close silently break a required write?** `close()` returns `()`, so it cannot report
   or swallow anything. Every assertion runs *before* the close. After the close the server is only
   idling on its listener; nothing in the test issues another request.

**Verified empirically, not assumed.** The first `ws_tests` run was executed under a hard 180-second
timeout guard because a hang was a real possibility. It completed in **1.13 s** with 0 artifacts.
Mutation 3 (§9) then proved the close is load-bearing in exactly this live-server setting.

---

## 6. Files changed

| file | added | changed | what |
|---|---:|---:|---|
| `process_lifetime_maps_tests.rs` | 10 | **0** | 10 × `state.store.pool.close().await;` |
| `key_scope_tests.rs` | 11 | **0** | 8 × close + 3 comment lines |
| `rbac_matrix_tests.rs` | 8 | **0** | 8 × close |
| `run_map_retention_tests.rs` | 6 | **1** | 6 × close, 1 × `store.clone()` |
| `ws_tests.rs` | 9 | **7** | 9 × close, 7 × `state.clone()` |
| **total** | **44** | **8** | |

**Three of five files are pure insertions** (pre-batch content is an ordered subsequence). The other
two have exactly the minimum changes needed to keep a teardown handle alive, each characterised at
character level:

```
run_map_retention_tests.rs
  OLD '        store,'            NEW '        store.clone(),'
     char delta at col 13: removed '' inserted '.clone()'

ws_tests.rs  (7 sites)
  OLD '    let base = spawn_server(state).await;'
  NEW '    let base = spawn_server(state.clone()).await;'
     char delta at col 33: removed '' inserted '.clone()'
```

Both are **ownership repairs, not behavioural changes**. In each case the original code moved the
handle away, making it impossible to reach the pool at teardown. `.clone()` is the idiom already used
throughout these files for exactly this (`state.clone()` appears elsewhere in `ws_tests` itself, and
every other `AppState`-consuming test clones its store). `Store::clone()` shares the pool, so the
object under test behaves identically.

---

## 7. Two ownership problems found by the compiler

Applying a uniform close produced two compile errors. Both were real and are documented here rather
than quietly patched.

### 7.1 `key_scope_tests` — the fixture lives inside a loop

```rust
for (label, body) in [("explicit-null", …), ("omitted", …)] {   // 2 iterations
    let (state, _dir) = test_state().await;                      // indent 8 — block-scoped
    …
}
state.store.pool.close().await;                                  // indent 4 — OUT OF SCOPE
```

`error[E0425]: cannot find value 'state' in this scope`. A close at the end of the test body was both
out of scope **and semantically wrong** — it would have covered only the last iteration. The close was
moved *inside* the loop, with a comment recording why:

```rust
// One fixture is built per loop iteration, so the close must sit inside the
// loop; at the end of the test body only the last iteration's store would be
// in scope, and the earlier one would keep its directory alive.
state.store.pool.close().await;
```

This is the source of the 41 → 42 discrepancy: one close statement, two stores, two directories.
A full-file scan for non-body-level fixture bindings found **exactly one** such site across all five
files.

### 7.2 `run_map_retention_tests` — the `Store` is moved into a constructor

```rust
let store = Store::connect(&dir.path().join("idem.sqlite3")).await.expect("connect");
let dispatcher = AgentExecutionDispatcher::new(
    store,                       // <-- moved
    …
);
…
store.pool.close().await;        // error[E0382]: borrow of moved value: `store`
```

Fixed with `store.clone()`, matching the codebase idiom. (The `store,` moves *inside* the five
`test_state()` helpers were checked and are fine — each moves into the `AppState` it returns.)

---

## 8. Repeated-run validation — 10 consecutive runs per suite

| suite | run | exit | passed | failed | new artifacts |
|---|---:|---:|---:|---:|---:|
| `process_lifetime_maps_tests` | 1–10 | 0 ×10 | 12 | 0 | **0 ×10** |
| `key_scope_tests` | 1–10 | 0 ×10 | 9 | 0 | **0 ×10** |
| `rbac_matrix_tests` | 1–10 | 0 ×10 | 15 | 0 | **0 ×10** |
| `run_map_retention_tests` | 1–10 | 0 ×10 | 6 | 0 | **0 ×10** |
| `ws_tests` | 1–10 | 0 ×10 | 9 | 0 | **0 ×10** |

**50 runs, 50 × zero new artifacts, zero teardown errors.** No run failed, so no failure evidence
needed preserving.

### 8.1 Per-test attribution

All **51** tests run **individually** with `--exact` and a before/after temp-directory snapshot:

```
tests run individually : 51
distinct exit codes    : 0
distinct newTmp values : 0
TOTAL new artifacts    : 0
zero-test filter traps : 0
```

---

## 9. Mutation validation

### Mutation 1 — remove all 41 teardown closes

* **Applied:** all five SHAs changed (`6f62bfff…`→`46f3523f…`, `85498461…`→`2eda34b2…`,
  `b795414c…`→`a7f6d847…`, `3256b1cb…`→`d2bf76e7…`, `4bb4b279…`→`433ed338…`). Exactly **41**
  removed, 0 remaining. Asserted in the script.
* **Rebuilt:** fresh `Compiling`, binary `13:51:03` newer than mutated source `13:50:22`, exit 0.

| suite | fixed | mutated | pre-batch |
|---|---:|---:|---:|
| `process_lifetime_maps_tests` | 0 | **10** | 10 |
| `key_scope_tests` | 0 | **9** | 9 |
| `rbac_matrix_tests` | 0 | **8** | 8 |
| `run_map_retention_tests` | 0 | **6** | 6 |
| `ws_tests` | 0 | **9** | 9 |
| **total** | **0** | **42** | **42** |

The leak returns **exactly**. And the 41-closes→42-directories relationship reappears under mutation,
independently confirming §7.1: the loop close is what covers the second iteration.

### Mutation 3 — targeted, on the live-server case

A bulk mutation cannot show *which* close matters. This one removed the single close from
`ws_tests::be14_subscribers_receive_only_their_own_projects_real_events` — the test with a live
`axum::serve` listener and a real WebSocket subscriber — and ran that one test with a fully-qualified
`--exact` filter.

* **Applied:** sha `4bb4b279…` → `79f1980d…`, exactly 1 line removed. Asserted.
* **Rebuilt:** fresh `Compiling`, binary newer than mutated source, exit 0.
* **Paired control on the same test:**

| condition | artifacts |
|---|---:|
| close present | **0** |
| close removed | **1** |
| close restored (byte-identical, `4bb4b279…`) | **0** |

Removing one close from one live-server test costs exactly one directory. The close is load-bearing
there, and the restore was SHA-verified.

### Mutation 2 — not applicable

No fault-injection close exists in these five families (§4). Documented as a limitation rather than
claimed as a success.

---

## 10. Regressions, formatting and Clippy

### 10.1 Regression sweep — 42 suites

| suite | exit | new artifacts | result |
|---|---:|---:|---|
| `process_lifetime_maps_tests` | 0 | **0** | 12 passed |
| `key_scope_tests` | 0 | **0** | 9 passed |
| `ws_tests` | 0 | **0** | 9 passed |
| `rbac_matrix_tests` | 0 | **0** | 15 passed |
| `run_map_retention_tests` | 0 | **0** | 6 passed |
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
| `store_tests` | 0 | 0 | 36 passed |
| `trace_tests` | 0 | 0 | 21 passed |
| `change_impact_tests` | 0 | 0 | 19 passed |
| `qa_memory_planning_tests` | 0 | 0 | 14 passed |
| `qa_memory_perf_tests` | 0 | 0 | 7 passed |
| `qa_memory_validation_tests` | 0 | 0 | 7 passed |
| `policy` | 0 | 0 | 7 passed |
| `project_stream` | 0 | 0 | 1 passed |
| `agent_execution` | 0 | 0 | 40 passed |
| `log_bus` | 0 | 0 | 1 passed |
| `trace_chain_tests` | 0 | 12 | 14 passed |
| `risk_signal_tests` | 0 | 11 | 18 passed |
| `journey_tests` | 0 | 10 | 13 passed |
| `integration_e2e_tests` | 0 | 17 | 20 passed |
| `operations_tests` | 0 | 15 | 15 passed |
| `adaptive_intelligence_tests` | 0 | 9 | 39 passed |
| `phase_7_1_regression_tests` | 0 | 11 | 15 passed |
| `phase_7_3_decision_tests` | 0 | 12 | 14 passed |
| `autopilot_runner` | 0 | 14 | 16 passed |
| `autopilot_steps` | 0 | 33 | 21 passed |
| `autopilot_execute` | 0 | 14 | 21 passed |
| `autopilot_generate` | 0 | 22 | 19 passed |
| `autopilot_report` | 0 | 2 | 17 passed |
| `store::history_tenant_tests` | 0 | 2 | 2 passed |

**42 suites, 0 exit failures, 0 reporting a real `test result: FAILED`** (checked with `-cmatch`;
plain `-match` is case-insensitive and matches the words `0 failed`).

**All 23 previously-fixed families remain at 0.** No regression anywhere.

### 10.2 Two measurement errors, caught and corrected

**A contaminated regression sweep.** The first sweep reported `process_lifetime_maps_tests` +10,
`key_scope_tests` +9, `rbac_matrix_tests` +8 and `run_map_retention_tests` +6 — i.e. exactly the
pre-batch figures. Cause: after Mutation 1 I restored only `ws_tests.rs` (Mutation 3's script
restores one file) and left the other four in their mutated state. `process_lifetime_maps_tests.rs`
had in fact reverted to byte-identical pre-batch content, SHA `46F3523FC3C8644D`. **The sweep was
discarded, all five files were restored from the `FIXED_` snapshots with SHA verification, and the
sweep was re-run** — that is the table above. The 50 repeated runs and 51 per-test runs in §8 predate
Mutation 1 and are unaffected.

**A PowerShell variable collision.** In Mutation 3's first measurement I assigned the fully-qualified
test name to `$t`; PowerShell variables are case-insensitive, so this **overwrote `$T`**, the
temp-directory root. `Get-ChildItem -Directory` then errored and both snapshots were empty, so two
empty sets compared equal and reported a confident `newTmp=0`. That reading was **invalid and
discarded**; the measurement was redone under a non-colliding variable name, with a snapshot-size
sanity check (3,023 entries) printed before the result was read.

**A broad-filter false alarm.** An exploratory `cargo test auth` reported +9 and looked like a new
family. It is the third broad-substring trap: the filter matched 14 tests across `api_tests`,
`store_tests`, `ws_tests`, `mobile::tests` and `tools::tests` purely because their *names* contain
"auth" (authentication / authorization / unauthorized). `auth.rs` itself has **no** inline tests, no
tempdir and no `Store::connect`. Not a finding.

### 10.3 Formatting

```
cargo fmt --check  →  exit 1
files rustfmt would change:  src/auth.rs:317
```

The sole complaint is the **pre-existing** rustfmt non-idempotency on `concat!` inside `role_gate!`,
left untouched by design. **None of the five Batch 9 files appears.** No file was passed through
write-mode rustfmt.

### 10.4 Clippy

```
cargo clippy --all-targets  →  exit 0
errors: 0    error[E]: 0    warnings: 66
```

`process_lifetime_maps_tests.rs` is named in 3 diagnostics. All three were checked against the
pre-batch file, which is **also** named in exactly 3 — so they are pre-existing (`unused_mut` at L110,
`assertions_on_constants` at L156, `unused variable: store` at L267) and **this batch introduced none**.
The other four files: 0 diagnostics.

---

## 11. Confirmations

| item | status |
|---|---|
| Production source untouched | `store.rs` = `97E79B8F876D7219`. Only the five Batch 9 **test** files have mtimes after 13:40. |
| `store::history_tenant_tests` | **not touched** — out of scope per instruction |
| Migrations / routes / schemas | none added, none changed |
| Existing user changes | preserved; 3 files pure-insertion, 2 with a single characterised `.clone()` each |
| Historical artifacts | **none deleted**; no cleanup script run |
| Commit / push | none. HEAD still `7930507`, 0/0 |
| Resets / stashes / `git clean` | none |
| Whole-file reformatting | none |
| Mutation residue in the tree | **none** (`PRE_<sha>_` backups live in `%TEMP%`, outside the repo) |
| Artifact growth 2,938 → 3,441 | a side-effect of ~170 measurement runs against deliberately unfixed families |
| Full backend suite | **not run**, per instruction |

---

## 12. Remaining fixture families and latest measurements

| remaining family | measured |
|---|---:|
| `api_tests` (6 fixture families; not run in this batch) | 308 *(previously measured)* |
| `autopilot_steps` | 33 |
| `autopilot_generate` | 22 |
| `integration_e2e_tests` | 17 |
| `operations_tests` | 15 |
| `autopilot_runner` | 14 |
| `autopilot_execute` | 14 |
| `phase_7_3_decision_tests` | 12 |
| `trace_chain_tests` | 12 |
| `risk_signal_tests` | 11 |
| `phase_7_1_regression_tests` | 11 |
| `journey_tests` | 10 |
| `adaptive_intelligence_tests` | 9 |
| `autopilot_report` | 2 |
| **`store::history_tenant_tests`** | **2** *(blocked — §13)* |

Not re-measured in this batch, carrying previous figures: `risk_semantics_tests`, `rbac_probe_tests`,
`pre_phase_8_tests`, `phase_7_4_decision_tests`, `retention_and_metrics_tests`.

**Accounting.** Batch 9 removed 42 of the 564 outstanding, leaving **522**. Of that, **184** was
re-measured here across 14 suites, **308** is `api_tests`, and roughly **30** sits in the five
unmeasured families. These are distinct quantities and are not summed into a percentage.

**Fixed to date:** 23 families, ~417 call sites, all at measured delta 0.

---

## 13. Outstanding blocker (unchanged, still out of scope)

**`store.rs:5888` — `#[cfg(test)] mod history_tenant_tests`.** 2 tests, 2 fixtures, 0 closes,
measured **+2**. It lives inside production `store.rs`, which is on the do-not-modify list with a
pinned SHA. **Not touched by this batch**, per instruction. It requires a separate authorization
decision and remains open.

---

## 14. Verdict

# Batch 9: PASS

Five families, **41 call sites covering 42 runtime fixtures**, **42 measured leaks eliminated**, all
five suites at **delta 0** across 50 repeated suite runs and 51 individual per-test runs. The
live-server case (`ws_tests`) verified under a timeout guard rather than assumed, and proved
load-bearing by a paired single-test mutation. No production file touched, no user work lost, no
artifact deleted. Two measurement errors and one false alarm were caught and corrected rather than
reported as findings. One pre-existing blocker remains, out of scope.

---

## 15. Recommendation for Batch 10

**Proceed with Batch 10** as scoped in the inventory, unchanged:

`trace_chain_tests`, `risk_signal_tests`, `risk_semantics_tests`, `rbac_probe_tests` — planned 30.

Carry forward, in priority order:

1. **Restore every mutated file before any measurement run, and verify all of them by SHA.** Two of
   three errors in this batch came from running measurements on a partly-mutated tree. Keep one
   `FIXED_` snapshot per file and restore the whole set together, not one file at a time.
2. **Check where a fixture is *bound*, not just that one exists.** A binding inside a loop or a
   nested block needs the close in the same block, or it covers only the last iteration — and one
   close statement can then be responsible for two directories.
3. **Check whether the fixture handle is moved.** `spawn_server(state)` and
   `AgentExecutionDispatcher::new(store, …)` both consume it. `.clone()` is the established idiom;
   prefer it over introducing a parallel teardown variable.
4. **Never use a bare module name as a test filter** unless it is a unique module. `auth`, `store`,
   `trace` and `change_impact` have all produced phantom leak figures this way. Use exact module
   paths, and sanity-check the match count against the expected test count.
5. **`trace_chain_tests` has a cross-module dependency on Batch 8.** Its test
   `a_fresh_migration_supplies_every_table_the_chain_traverses` calls
   `crate::trace_tests::temp_store_for_phase2()`. Fixing `trace_tests::temp_store` did **not** fix
   that call site — the wrapper hands ownership to the caller. It must be closed in
   `trace_chain_tests` itself.

> **Phase 9.2 remains OPEN.** It closes only when every identified fixture family is fixed with a
> measured delta of 0 or proven safe — **including `store::history_tenant_tests`, which needs a
> production-file authorization decision** — and the final full-backend-suite artifact verification
> confirms no new artifacts accumulate. 522 remain. Batch 10 has not been started. Phase 10 has not
> been started.
