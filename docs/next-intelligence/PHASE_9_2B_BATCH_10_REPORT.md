# Phase 9.2B — Batch 10 Report

## Trace-Chain, Risk-Signal, Risk-Semantics and RBAC-Probe Fixtures

**Date:** 2026-10-11
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind) — nothing committed, nothing pushed
**Verdict: PASS**

---

## 0. Headline numbers

| | |
|---|---:|
| Unique fixture helpers fixed | **4** |
| Call sites fixed | **30** (estimate was 30) |
| Tests owning a fixture | **30** of 59 |
| Measured leaks eliminated | **30** |
| Post-fix delta, all four suites | **0** |
| Existing `pool.close()` in scope | **0** — none existed (§4) |
| Repeated suite runs | **40** (10 per suite), all exit 0, all delta 0 |
| Individual per-test runs | **59**, all exit 0, all delta 0 |
| Files changed | 4 test files, **30 lines added, 0 changed** — all pure insertions |
| Production files touched | **0** |

---

## 1. Initial repository state

| item | value |
|---|---|
| Branch / HEAD | `main` / `7930507` |
| Upstream | 0 ahead / 0 behind |
| Modified / Untracked / Deleted / Staged | 35 / 38 / **0** / 0 |
| `.tmp*` directories | 3,441 |
| `C:` free | 39.89 GB |
| `store.rs` SHA (production, pinned) | `97E79B8F876D7219` — unchanged throughout |

Pre-batch pins, backed up to `%TEMP%\B10_backup` before any edit:

| file | sha256 | lines |
|---|---|---:|
| `trace_chain_tests.rs` | `E46546E556DFB07E` | 1,118 |
| `risk_signal_tests.rs` | `F4CEEB9C514C8498` | 1,263 |
| `risk_semantics_tests.rs` | `1D56103A38630D57` | 612 |
| `rbac_probe_tests.rs` | `58375324D81774A5` | 635 |

All four are standalone `src/*.rs` test modules. **None is inline in a production file**, so both
known production-file blockers are outside this batch's edit surface.

---

## 2. Exact counts per family

| family | file | helper | tests | tests owning a fixture | call sites | close statements |
|---|---|---|---:|---:|---:|---:|
| `trace_chain_tests` | `trace_chain_tests.rs` | `harness()` → `Harness` | 14 | 12 | **12** | 12 |
| `risk_signal_tests` | `risk_signal_tests.rs` | `harness()` → `(Store, TempDir)` | 18 | 11 | **11** | 11 |
| `risk_semantics_tests` | `risk_semantics_tests.rs` | `harness()` → `(Store, TempDir)` | 10 | 3 | **3** | 3 |
| `rbac_probe_tests` | `rbac_probe_tests.rs` | `harness()` → `Harness` | 17 | 4 | **4** | 4 |
| **total** | | | **59** | **30** | **30** | **30** |

This batch is the first where **call sites, fixture-owning tests and close statements are all exactly
equal at 30** — a clean 1:1:1 with no loops, no second bindings and no inline fixtures. (Batch 9 was
41 → 42 because one close sat inside a loop and ran twice.)

### 2.1 Fixture shapes

```rust
// trace_chain_tests.rs:63
struct Harness { store: Store, healing: GovernedHealingService, _dir: tempfile::TempDir }
// risk_signal / risk_semantics
async fn harness() -> (Store, tempfile::TempDir)
// rbac_probe_tests.rs:247
struct Harness { state: AppState, _dir: tempfile::TempDir }
```

DB names: `phase2-chain.sqlite3`, `phase4-signal-remediation.sqlite3`, `phase61-semantics.sqlite3`,
`probe.sqlite3`.

The four harnesses therefore need four different close expressions, which is why the edit was
rule-driven per file rather than a single textual replacement:

| file | close expression | count |
|---|---|---:|
| `trace_chain_tests.rs` | `h.store.pool.close().await;` | 11 |
| `trace_chain_tests.rs` | `store.pool.close().await;` (cross-module) | 1 |
| `risk_signal_tests.rs` | `store.pool.close().await;` | 11 |
| `risk_semantics_tests.rs` | `store.pool.close().await;` | 3 |
| `rbac_probe_tests.rs` | `h.state.store.pool.close().await;` | 4 |

`trace_chain_tests::harness()` clones its `store` into both the `AgentExecutionDispatcher` and the
`GovernedHealingService`. `Store::clone()` shares one pool, so closing `h.store.pool` releases every
handle those services hold.

---

## 3. `trace_chain_tests` — the shared-helper caller (Step 2)

This was the batch's specific requirement, and it behaved exactly as Batch 9 predicted.

```rust
// trace_tests.rs:45  (fixed in Batch 8)
pub async fn temp_store_for_phase2() -> (Store, tempfile::TempDir) { temp_store().await }

// trace_chain_tests.rs:1060  — the caller
#[tokio::test]
async fn a_fresh_migration_supplies_every_table_the_chain_traverses() {
    let (store, _d) = crate::trace_tests::temp_store_for_phase2().await;   // ownership transfers HERE
    let applied: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&store.pool).await.expect("migration count");
    assert_eq!(applied, 32, "…");
    …
    store.pool.close().await;      // added by this batch
}
```

**Return type and ownership.** `temp_store_for_phase2()` returns `(Store, tempfile::TempDir)` by
value. The caller destructures both and therefore owns both. `trace_tests`' Batch 8 teardown sits at
the end of *`trace_tests`' own tests* — it runs nowhere near this caller.

**Every pool used by this test:** exactly one — the `Store` it received. No second `Store` is
connected to the same file; `Store::connect` appears only inside `trace_chain_tests::harness()` at
L71, and this test does not call that.

**Preserved the shared helper.** `trace_tests.rs` was **not modified in this batch** (SHA still
`C62C6F0D4AFA4CCA`). `temp_store_for_phase2` is a correct helper; the defect was the caller's missing
teardown, and the fix belongs at the call site.

**Proof this was necessary** is Mutation 2 in §9: with `trace_tests::temp_store` already fixed in
Batch 8, removing only this caller's own close still leaks exactly one directory.

---

## 4. Classification of existing pool closes (Step 3)

| file | existing closes | classification |
|---|---:|---|
| `trace_chain_tests.rs` | **0** | — |
| `risk_signal_tests.rs` | **0** | — |
| `risk_semantics_tests.rs` | **0** | — |
| `rbac_probe_tests.rs` | **0** | — |

**No existing closures of any kind in any of the four families.** Nothing to classify, nothing to
preserve, no `UNRESOLVED` sites, and no multi-pool/second-Store-on-one-file cases. There is exactly
one `Store::connect` per file, inside that file's own `harness()`, except `trace_chain_tests`, which
has one inside `harness()` and one instance received via the shared helper in a test that does not
call `harness()`.

---

## 5. Ownership and background-work analysis

### 5.1 Direct background work: none

None of the four files contains `tokio::spawn`, `spawn_blocking`, `JoinHandle`, `thread::spawn`,
`spawn_run`, `axum::serve`, `Instant::now` or any sleep. **No test in this batch relies on a sleep,
and none depends on asynchronous telemetry or persisted evidence arriving late.**

### 5.2 Indirect background work — production callees inspected

Per the brief's warning not to infer "no background work" from the absence of `tokio::spawn` in a test
file, each production callee the tests drive was checked directly:

| production file | spawn primitives | note |
|---|---|---|
| `intelligence.rs` | **none** (0 `tokio::spawn`, 0 `spawn_blocking`, 0 `JoinHandle`, 0 `spawn_run`, 0 `.dispatch(`) | `Intelligence::new(store: &'a Store)` **borrows** the store; it holds no ownership that could outlive a close |
| `governed_healing.rs` | **none** | `GovernedHealingService`, used by `trace_chain_tests::harness()` |
| `rbac_probe.rs` | **none** | used by `rbac_probe_tests` |
| `agent_execution.rs` | `engine::spawn_run(…, self.store.clone(), …)` at L403 | the one real detached-dispatch path in the transitive closure |

`agent_execution.rs:403` is a genuine fire-and-forget dispatch holding a `Store` clone. **It is
unreachable from these four families**: it sits inside `AgentExecutionDispatcher::dispatch(..)`, and
none of the four files calls `.dispatch(`, `dispatcher.` or `spawn_run`. A full-text search of all
four files for `\.dispatch\(|spawn_run|dispatcher\.` returns zero hits. `governed_healing.rs` — which
*does* own a dispatcher inside `trace_chain_tests::harness()` — contains no spawn primitive of its
own and is only ever used through the methods the tests actually call.

### 5.3 Three false-positive "moves" corrected before editing

A scan flagged `Intelligence::new(store)` at `trace_chain_tests:190`, `risk_signal_tests:146` and
`risk_semantics_tests:592` as moves. Reading `intelligence.rs:243`:

```rust
pub fn new(store: &'a Store) -> Self { Self { store } }   // pub struct Intelligence<'a> { pub store: &'a Store }
```

It **borrows**. All three sites are inside test *helpers* (`ingest_failing_run`, `run`) that take
`&Store`, not inside tests. **Zero real moves exist in this batch**, which is why the final diff is
four pure insertions with no `.clone()` repairs — unlike Batches 8 and 9.

---

## 6. Files changed

| file | added | removed | changed | what |
|---|---:|---:|---:|---|
| `trace_chain_tests.rs` | 12 | 0 | **0** | 11 × `h.store.pool…` + 1 × `store.pool…` (cross-module) |
| `risk_signal_tests.rs` | 11 | 0 | **0** | 11 × `store.pool…` |
| `risk_semantics_tests.rs` | 3 | 0 | **0** | 3 × `store.pool…` |
| `rbac_probe_tests.rs` | 4 | 0 | **0** | 4 × `h.state.store.pool…` |
| **total** | **30** | **0** | **0** | |

**All four files are pure insertions** — each file's pre-batch content is a verified ordered
subsequence of its post-batch content, with zero changed lines. This is the first batch since 9.2A
where that is true for every file.

---

## 7. Per-suite artifact measurements

| family | pre-batch | **post-batch** | change |
|---|---:|---:|---|
| `trace_chain_tests` | 12 | **0** | −12 |
| `risk_signal_tests` | 11 | **0** | −11 |
| `risk_semantics_tests` | 3 | **0** | −3 |
| `rbac_probe_tests` | 4 | **0** | −4 |
| **total** | **30** | **0** | **−30** |

Both figures measured in this session; the pre-batch column reproduces the inventory's "+30" exactly.

### 7.1 Per-test attribution

All **59** tests run **individually** with a fully-qualified `--exact` filter and a before/after
temp-directory snapshot:

```
sanity: snapshot entries = 3471
tests run individually : 59
distinct exit codes    : 0
distinct newTmp values : 0
TOTAL new artifacts    : 0
zero-test filter traps : 0
```

---

## 8. Repeated-run validation — 10 consecutive runs per suite

| suite | run | exit | passed | failed | new artifacts |
|---|---:|---:|---:|---:|---:|
| `trace_chain_tests` | 1–10 | 0 ×10 | 14 | 0 | **0 ×10** |
| `risk_signal_tests` | 1–10 | 0 ×10 | 18 | 0 | **0 ×10** |
| `risk_semantics_tests` | 1–10 | 0 ×10 | 10 | 0 | **0 ×10** |
| `rbac_probe_tests` | 1–10 | 0 ×10 | 17 | 0 | **0 ×10** |

**40 runs, 40 × zero new artifacts, zero teardown errors.** No run failed, so no failure evidence
needed preserving.

---

## 9. Mutation validation

### Mutation 1 — remove all 30 teardown closes

* **Applied:** all four SHAs changed, and each mutated SHA **equals its original pre-batch SHA**
  (`8850e748…`→`e46546e5…`, `1ead13f8…`→`f4ceeb9c…`, `4c4d8a9a…`→`1d56103a…`,
  `e0e9bb5a…`→`58375324…`). Exactly **30** removed, 0 remaining. Asserted.
  The SHA returning *exactly* to the pre-batch value is stronger evidence than a diff count: it shows
  the mutation added and removed nothing but the 30 close lines.
* **Rebuilt:** fresh `Compiling`, binary `14:05:55` newer than mutated source `14:05:25`, exit 0.

| suite | fixed | mutated | pre-batch |
|---|---:|---:|---:|
| `trace_chain_tests` | 0 | **12** | 12 |
| `risk_signal_tests` | 0 | **11** | 11 |
| `risk_semantics_tests` | 0 | **3** | 3 |
| `rbac_probe_tests` | 0 | **4** | 4 |
| **total** | **0** | **30** | **30** |

### Mutation 2 — the cross-module caller's close specifically (Step 9)

Removes *only* the close at `trace_chain_tests.rs:1128`, inside
`a_fresh_migration_supplies_every_table_the_chain_traverses` — the one test that builds its fixture
through `crate::trace_tests::temp_store_for_phase2()` rather than through its own `harness()`.

* **Applied:** sha `8850e748…` → `8faeb639…`, exactly 1 line removed. Asserted.
* **Rebuilt:** fresh `Compiling`, binary newer than mutated source, exit 0.
* **Run:** fully-qualified filter
  `trace_chain_tests::a_fresh_migration_supplies_every_table_the_chain_traverses --exact`,
  reporting `0 passed; 1 failed; 0 ignored; 2157 filtered out` — the test genuinely executed.

| condition | artifacts |
|---|---:|
| close present | **0** |
| close removed | **1** |
| close restored (byte-identical, `8850e748…`) | **0** |

**This is the direct answer to Step 9's requirement.** `trace_tests::temp_store` was already fixed in
Batch 8 and measures 0 in `trace_tests` itself — yet removing only the caller's own close still leaks
one directory. The shared helper performs **no** cleanup on behalf of its callers; ownership of the
returned `(Store, TempDir)` transfers to the caller, and only the caller's teardown can release it.

### Mutation 3 — fault injection: not applicable

No fault-injection close exists in these four families (§4). Documented as a limitation rather than
claimed as coverage.

---

## 10. Regression checks

### 10.1 Sweep — 40 suites

| suite | exit | new artifacts | result |
|---|---:|---:|---|
| `trace_chain_tests` | 0 | **0** | 14 passed |
| `risk_signal_tests` | 0 | **0** | 18 passed |
| `risk_semantics_tests` | 0 | **0** | 10 passed |
| `rbac_probe_tests` | 0 | **0** | 17 passed |
| `trace_tests` (Batch 8, the shared helper) | 0 | 0 | 21 passed |
| `change_impact_tests` | 0 | 0 | 19 passed |
| `store_tests` | 0 | 0 | 36 passed |
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
| `qa_memory_planning_tests` | 0 | 0 | 14 passed |
| `qa_memory_perf_tests` | 0 | 0 | 7 passed |
| `qa_memory_validation_tests` | 0 | 0 | 7 passed |
| `rbac_matrix_tests` | 0 | 0 | 15 passed |
| `key_scope_tests` | 0 | 0 | 9 passed |
| `ws_tests` | 0 | 0 | 9 passed |
| `process_lifetime_maps_tests` | 0 | 0 | 12 passed |
| `run_map_retention_tests` | 0 | 0 | 6 passed |
| `journey_tests` | 0 | 10 | 13 passed |
| `integration_e2e_tests` | 0 | 16 | 20 passed |
| `operations_tests` | 0 | 15 | 15 passed |
| `adaptive_intelligence_tests` | 0 | 9 | 39 passed |
| `phase_7_1_regression_tests` | 0 | 11 | 15 passed |
| `phase_7_3_decision_tests` | 0 | 12 | 14 passed |
| `autopilot_runner` | 0 | 15 | 16 passed |
| `autopilot_steps` | 0 | 31 | 21 passed |
| `autopilot_execute` | 0 | 14 | 21 passed |
| `autopilot_generate` | 0 | 23 | 19 passed |
| `autopilot_report` | 0 | 2 | 17 passed |
| `store::history_tenant_tests` | 0 | 2 | 2 passed |

**40 suites, 0 exit failures, 0 reporting a real `test result: FAILED`** (checked with `-cmatch`;
plain `-match` is case-insensitive and matches the words `0 failed`).

**All 27 previously-fixed families remain at 0.** Traceability, project isolation, risk and RBAC
regressions (`trace_tests`, `change_impact_tests`, `rbac_matrix_tests`, `key_scope_tests`,
`ws_tests`, `integrity_tests`) are all unchanged and clean.

### 10.2 A broad-filter false alarm — and a real finding behind it

An exploratory `cargo test intelligence` reported **+19 across 70 tests**. Checking what that filter
actually matched:

| matched module | tests |
|---|---:|
| `adaptive_intelligence_tests` | 39 |
| `intelligence` (inline in `intelligence.rs`) | 25 |
| `api_tests` | 5 |
| `journey_tests` | 1 |
| `agents` | 1 |

The +19 is an aggregate across unrelated suites — a broad-filter artifact, not one family's figure.
Re-measured with exact filters:

| filter | new artifacts | tests |
|---|---:|---|
| `intelligence::` | **2** | 24 passed, 1 ignored |
| `adaptive_intelligence_tests` | 9 | 39 passed |

The `intelligence::` figure is **real** and is recorded as a new blocker in §12. The filter itself is
the fourth broad-substring trap after `auth`, `store` and `trace`/`change_impact`.

### 10.3 Formatting

```
cargo fmt --check  →  exit 1
files rustfmt would change:  src/auth.rs:317
```

The sole complaint is the **pre-existing** rustfmt non-idempotency on `concat!` inside `role_gate!`,
left untouched by design. **None of the four Batch 10 files appears.** No file was passed through
write-mode rustfmt.

### 10.4 Clippy

```
cargo clippy --all-targets  →  exit 0
errors: 0    error[E]: 0    warnings: 66
```

`risk_signal_tests.rs` is named in 1 diagnostic and `rbac_probe_tests.rs` in 2. Both were re-checked
against the **pre-batch** files, which are named in the **same** counts (1 and 2) — so all three are
pre-existing and this batch introduced none. `trace_chain_tests.rs` and `risk_semantics_tests.rs`:
0 diagnostics.

---

## 11. Confirmations

| item | status |
|---|---|
| Production source untouched | `store.rs` = `97E79B8F876D7219`; `intelligence.rs` = `B8051724D69E2E40`; `trace_tests.rs` = `C62C6F0D4AFA4CCA`. Only the four Batch 10 **test** files have mtimes after 13:55. |
| Shared helper `temp_store_for_phase2` | **preserved unchanged** — `trace_tests.rs` untouched |
| `store.rs` / `history_tenant_tests` | **not touched**, per instruction |
| Risk formulas, weights, thresholds, severity classification | **none altered** — this batch changed only resource lifecycle |
| Migrations / routes / schemas | none added, none changed |
| Existing user work | preserved; four pure insertions, zero changed lines |
| Historical artifacts | **none deleted**; no cleanup script run |
| Commit / push | none. HEAD still `7930507`, 0/0 |
| Resets / stashes / `git clean` | none |
| Whole-file reformatting | none |
| Mutation residue | **none** (`PRE_<sha>_` backups live in `%TEMP%`, outside the repo) |
| Artifact growth 3,441 → 3,692 | measurement side-effect only, against deliberately unfixed families |
| Full backend suite | **not run**, per instruction |

---

## 12. Blockers

### 12.1 `store.rs:5888` — `mod history_tenant_tests` (unchanged, out of scope)

2 tests, 2 fixtures, 0 closes, **+2**. Inside production `store.rs`. Untouched; still requires a
separate authorization decision.

### 12.2 `intelligence.rs:982` — `mod …` inline tests (**newly discovered**)

Found by this batch's regression sweep and confirmed by exact-filter measurement.

```rust
// intelligence.rs:982
#[cfg(test)]
mod … {
    …
    #[tokio::test]                                                     // L1152
    async fn link_test_case_to_requirement_reflects_real_edges_and_respects_project_isolation() {
        let dir = tempfile::tempdir().expect("tempdir");                // L1154
        let store = crate::store::Store::connect(&dir.path().join("intel.sqlite3"))  // L1155
            .await.expect("connect");
        let intel = Intelligence::new(&store);
        …
```

* **25 inline test attributes, 24 run (1 `#[ignore]`d), 2 inline fixtures** at L1154 and L1260, each
  with its own `tempdir()` + `Store::connect`, and **0 existing `pool.close()`**.
* Measured **+2** with the exact filter `intelligence::`.
* Like `history_tenant_tests`, it lives inside a **production file** (`intelligence.rs`) and cannot be
  fixed without authorization. **Not touched.**

Together these are **two production-file blockers, +4 combined**, both requiring a decision outside
the test-only batches.

---

## 13. Remaining fixture families and latest measurements

| remaining family | measured |
|---|---:|
| `api_tests` (6 fixture families; not run in this batch) | 308 *(previously measured)* |
| `autopilot_steps` | 31 |
| `autopilot_generate` | 23 |
| `integration_e2e_tests` | 16 |
| `operations_tests` | 15 |
| `autopilot_runner` | 15 |
| `autopilot_execute` | 14 |
| `phase_7_3_decision_tests` | 12 |
| `phase_7_1_regression_tests` | 11 |
| `journey_tests` | 10 |
| `adaptive_intelligence_tests` | 9 |
| `autopilot_report` | 2 |
| **`store::history_tenant_tests`** | **2** *(blocked, §12.1)* |
| **`intelligence::` (inline)** | **2** *(blocked, §12.2 — newly discovered)* |

Not re-measured in this batch, carrying previous figures: `pre_phase_8_tests`,
`phase_7_4_decision_tests`, `retention_and_metrics_tests`.

### 13.1 Corrected inventory totals

Batch 9 reported 522 outstanding after its own work. That figure was built from the Batch 8
reconciliation, which did not yet account for the newly found `intelligence::` blocker:

| component | value |
|---|---:|
| 564 after Batch 8 | 564 |
| − Batch 9 (measured) | −42 |
| = after Batch 9 | 522 |
| − Batch 10 (measured) | −30 |
| **= after Batch 10** | **492** |
| of which `api_tests` | 308 |
| of which two production-file blockers | 4 |
| of which re-measured remaining suites | 151 |
| of which carried-forward unmeasured (`pre_phase_8`, `phase_7_4`, `retention_and_metrics`) | ~29 |

**Fixed to date: 27 families, ~447 call sites, all at measured delta 0.**

These are distinct quantities — call sites, fixture-owning tests, measured directory deltas and
family counts — and are not summed into a completion percentage.

---

## 14. Verdict

# Batch 10: PASS

Four families, **30 call sites, 30 fixture-owning tests, 30 close statements, 30 measured leaks
eliminated** — a clean 1:1:1:1. All four suites at **delta 0** across 40 repeated suite runs and 59
individual per-test runs. Four files changed, **30 lines added, 0 removed, 0 changed** — all pure
insertions. The cross-module helper requirement is satisfied and *proven necessary* by a paired
single-test mutation. No production file touched, no user work lost, no artifact deleted.

One new blocker found (§12.2) requiring authorization this batch does not have.

---

## 15. Recommendation for Batch 11

**Proceed with Batch 11** as scoped in the inventory, unchanged:

`phase_7_1_regression_tests` (remaining scope), `phase_7_3_decision_tests`, `pre_phase_8_tests`,
`phase_7_4_decision_tests` — planned 45 in total; note `phase_7_1` and `phase_7_3` were re-measured
this batch at 11 and 12, so the live portion is `pre_phase_8_tests` + `phase_7_4_decision_tests` plus
their fault-injection sites, which must be classified before any edit.

Carry forward:

1. **Restore every mutated file together, and verify all of them by SHA, before any measurement.**
   Batch 9 produced a contaminated sweep this way. This batch snapshotted and restored all four files
   as a set and had no recurrence.
2. **Check whether a callee *borrows* or *owns* the store before assuming a move.** `Intelligence::new`
   takes `&Store`; `TraceReader::new`, `AgentExecutionDispatcher::new` and `spawn_server` do not.
3. **Never use a bare module name as a filter.** `auth`, `store`, `trace`, `change_impact` and
   `intelligence` have all produced phantom figures this way.
4. **A fixture helper that hands `(Store, TempDir)` to a caller transfers ownership.** The caller owes
   the close, even when the helper itself is already fixed. This is now proven, not assumed.

> **Phase 9.2 remains OPEN.** It closes only when every identified fixture family is fixed with a
> measured delta of 0 or proven safe — **including the two production-file blockers
> (`store::history_tenant_tests` +2 and `intelligence::` +2), which need an authorization decision** —
> and the final full-backend-suite artifact verification confirms no new artifacts accumulate. 492
> remain. Batch 11 has not been started. Phase 10 has not been started.
