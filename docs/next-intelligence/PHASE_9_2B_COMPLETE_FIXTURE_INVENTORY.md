# Phase 9.2B — Complete Fixture Inventory

**AUDIT ONLY. No code was changed.**

**Date:** 2026-10-11
**Branch / HEAD:** `main` @ `7930507` (0 ahead / 0 behind) — unchanged, nothing committed
**Scope:** every test-fixture family in the crate that can leave a SQLite file, or its WAL/SHM
sidecars, behind after a test completes.

---

## 1. Initial repository state

| item | value |
|---|---|
| Branch / HEAD | `main` / `7930507` |
| Upstream | 0 ahead / 0 behind |
| Modified / Untracked / Deleted / Staged | 28 / 33 / **0** / 0 |
| `.tmp*` directories at audit start | **1,248** |
| `C:` free at audit start | **54.62 GB** |
| Rust source files | **112** |
| `#[cfg(test)]` modules declared in `lib.rs` | **40** |

All ten previously-fixed/authoritative files were SHA-pinned before the audit began and re-verified
afterwards — **every one unchanged**:

| file | SHA-256 (first 16) | at end of audit |
|---|---|---|
| `store.rs` | `97E79B8F876D7219` | unchanged |
| `integrity.rs` | `EAEA60CFA367BCB6` | unchanged |
| `integrity_tests.rs` | `CC1205A7FBB688A6` | unchanged |
| `phase_8_quality_tests.rs` | `B5D3CA6E616187F1` | unchanged |
| `phase_9_readiness_tests.rs` | `99D402A29E57EDCF` | unchanged |
| `exploration_tests.rs` | `8A8980B655B0C2D5` | unchanged |
| `agent_execution_tests.rs` | `6CAF99485C2CCE4B` | unchanged |
| `governed_healing_tests.rs` | `08F014852495130F` | unchanged |
| `engine_tests.rs` | `180C9F31F92A0F63` | unchanged |
| `qa_memory_tests.rs` | `39E4E68CEF0BF64D` | unchanged |

### 1.1 Artifacts

Per the brief's historical note, the directories removed between earlier sessions **no longer exist**
and this report makes no claim that they do. **Nothing was deleted during this audit.** Artifact count
rose 1,248 → 1,800 purely as a *measurement side-effect* of running the leaking suites in §6; free
disk went 54.62 → 53.99 GB.

**The full backend suite was deliberately not run.** It is known to create substantial artifacts
while remaining fixtures leak, and running it would not have improved attribution.

---

## 2. Whole-source-tree search methodology

### 2.1 What was searched

All **112** `.rs` files under `services/orchestrator/src`, searched for every construction pattern that
can create a SQLite-backed temporary directory:

| pattern | purpose |
|---|---|
| `tempfile::tempdir()` | primary fixture tempdir |
| `tempdir_in(...)` | isolated-root variants added by this work |
| `TempDir::new()` | **a third form — see §2.2** |
| `Store::connect(...)` | where a pool is actually opened |
| `AppState::new(...)` | `api_tests`-family state construction |
| `pool.close()` | every existing close, for purpose classification |
| `tokio::spawn`, `spawn_blocking`, `JoinHandle`, `spawn_run`, `engine::`, `axum::serve`, `TcpListener::bind` | background-execution reachability |
| functions returning `(Store, TempDir)`, `(AppState, TempDir)`, `Db`, service bundles | fixture return shapes |

Call sites were counted **within the defining file** (these helpers are module-private), because
identically-named helpers in different files are distinct families. Earlier batch reports conflated
same-named helpers across files; this audit does not.

### 2.2 Two methodology defects I found and corrected in my own scan

These matter because each one **hid a real leaking family**:

1. **Zero-argument-only call-site matching missed parameterised fixtures.**
   `api_tests::test_state_with_ai_router(ai_router)` takes an argument, so a `fn name()` pattern
   reported it as having **0 call sites**. It actually has **114**.
2. **`tempfile::tempdir()` / `tempdir()` / `tempdir_in()` missed `TempDir::new()`.**
   `adaptive_intelligence_tests` and `operations_tests` both build fixtures with `TempDir::new()`,
   so a sweep keyed on the `tempdir` family reported **no fixture at all** for those two files. Each
   has a real leaking helper.

Corrected totals: **76 tempdir-construction statements across 44 files** (first pass found 72 across
41). Both corrections are folded into §3.

### 2.3 A third defect, carried in from Batch 6

Locating a function's end by counting `{`/`}` **breaks on string literals containing braces**. This
produced a wrong insertion in Batch 6 (12 of 24 sites) and wrong counts in an earlier scanner. All
function-boundary work in this audit uses the **column-0 closing brace** or **`#[tokio::test]`
segment boundaries**, which are immune to braces inside strings.

---

## 3. Complete fixture-helper inventory

### 3.1 Already fixed — verified leak-free (8 families)

Each measured at **delta 0** across repeated runs and, where applicable, per-test attribution.

| # | helper | file | call sites | status |
|---|---|---|---:|---|
| 1 | `harness` | `integrity_tests.rs` | 28 | fixed 9.2A |
| 2 | `Db::new` | `phase_8_quality_tests.rs` | 15 | fixed Batch 1 |
| 3 | `Db::new` | `phase_9_readiness_tests.rs` | 16 | fixed Batch 1 |
| 4 | `harness` | `exploration_tests.rs` | 19 | fixed Batch 2 |
| 5 | `setup_harness` | `agent_execution_tests.rs` | 15 | fixed Batch 3 |
| 6 | `setup_healing_harness` | `governed_healing_tests.rs` | 43 | fixed Batch 4 |
| 7 | `temp_store` | `engine_tests.rs` | 33 | fixed Batch 5 |
| 8 | `temp_store` | `qa_memory_tests.rs` | 24 | fixed Batch 6 |

Plus one **inline** fixture repaired in Batch 6: `migrations_preserve_rows_written_before_them` in
`qa_memory_tests.rs` opened two pools on one file.

**Total migrated call sites: 193.**

### 3.2 Remaining — measured leaking (29 families)

Every row below is a **measured** delta from a suite run in this audit (§6), not an estimate.

| # | helper | file | sites | measured delta | bg-work risk | 2nd-pool risk |
|---|---|---|---:|---:|---|---|
| 1 | `test_state` | `api_tests.rs` | 116 | **308** (shared, §9) | none found | yes |
| 2 | `test_state_with_compiled_api` | `api_tests.rs` | 66 | ↑ shared | none found | yes |
| 3 | `test_state_with_ai_router` | `api_tests.rs` | 114 | ↑ shared | none found | yes |
| 4 | `test_state_with_ai_router_and_embedder` | `api_tests.rs` | 11 | ↑ shared | none found | yes |
| 5 | `test_state_with_playwright_stub` | `api_tests.rs` | 3 | ↑ shared | none found | yes |
| 6 | `test_state_with_marker_executor` | `api_tests.rs` | 1 | ↑ shared | none found | yes |
| 7 | `temp_store` | `store_tests.rs` | 34 | **36** | not checked | no |
| 8 | `temp_store` | `qa_memory_retrieval_tests.rs` | 20 | **21** | none found | no |
| 9 | `temp_store` | `autopilot_store.rs` | 20 | **21** | not checked | no |
| 10 | `temp_store` | `qa_memory_risk_tests.rs` | 20 | **19** | none found | no |
| 11 | `temp_store` | `risk_prioritisation_tests.rs` | 20 | **19** | none found | no |
| 12 | `temp_store` | `trace_tests.rs` | 18 | **18** | none found | no |
| 13 | `new` | `integration_e2e_tests.rs` | 29 | **16** | not checked | no |
| 14 | `setup_test_store` | `operations_tests.rs` | 15 | **15** | none found | no |
| 15 | `temp_store` | `change_impact_tests.rs` | 16 | **14** | none found | no |
| 16 | `temp_store` | `qa_memory_planning_tests.rs` | 13 | **14** | none found | no |
| 17 | `new` | `phase_7_3_decision_tests.rs` | 17 | **12** | none found | no |
| 18 | `new` | `pre_phase_8_tests.rs` (2 defs) | 17 | **12** | none found | no |
| 19 | `harness` | `trace_chain_tests.rs` | 10 | **12** | none found | no |
| 20 | `new` | `phase_7_1_regression_tests.rs` | 29 | **11** | none found | no |
| 21 | `harness` | `risk_signal_tests.rs` | 10 | **11** | none found | no |
| 22 | `new` | `phase_7_4_decision_tests.rs` | 14 | **10** | none found | no |
| 23 | `test_state` | `process_lifetime_maps_tests.rs` | 9 | **10** | none found | no |
| 24 | `new` | `journey_tests.rs` | 37 | **9** | not checked | no |
| 25 | `setup_test_store` | `adaptive_intelligence_tests.rs` | 10 | **9** | none found | no |
| 26 | `test_state` | `key_scope_tests.rs` | 7 | **9** | none found | no |
| 27 | `test_state` | `ws_tests.rs` | 8 | **9** | none found | no |
| 28 | `test_state` | `rbac_matrix_tests.rs` | 7 | **8** | none found | no |
| 29 | `temp_store` | `qa_memory_perf_tests.rs` | 6 | **7** | none found | no |
| 30 | `temp_store` | `qa_memory_validation_tests.rs` | 6 | **7** | none found | no |
| 31 | `test_state` | `run_map_retention_tests.rs` | 4 | **6** | none found | no |
| 32 | `harness` | `rbac_probe_tests.rs` | 3 | **4** | none found | no |
| 33 | `harness` | `risk_semantics_tests.rs` | 2 | **3** | none found | no |
| 34 | *(inline)* | `retention_and_metrics_tests.rs` | — | **10** | none found | no |

**Measured total across all remaining suites: +660 directories per full run.**

### 3.3 Remaining — NOT measured (excluded from the total)

| item | reason |
|---|---|
| `autopilot_runner::new` (18 sites, L458) | helper is in **production** code; inline tests measured separately (`autopilot_runner`, not run) |
| `autopilot_test_support::fixture` (pub(crate), 0 direct sites) | shared support used by other modules; ownership not established |
| `api_tests` 4 inline-tempdir restart/crash tests | folded into the api_tests figure |
| `qa_memory_migration_tests` (1 test) | **measured 0 — does not leak** |
| `store.rs::temp_store` (1 inline test) | production-file inline test, not run |

---

## 4. Exact call-site and test counts

**These are two different quantities and must not be summed.**

* **Unique remaining fixture helpers: 34** (32 helper functions + `api_tests`' 6 families counted as 6, plus inline).
  Precisely: **33 named helper functions** across **30 files**, plus **4 inline fixtures** inside
  `api_tests`/`retention_and_metrics_tests`.
* **Remaining call sites (source-counted): ~530**, of which **311 are `api_tests`**.
* **Measured leaking directories per full run: +660.**

The site count and the directory count differ because a family can leak more or fewer directories
than it has sites (e.g. `integration_e2e_tests` has 29 sites but measured 16; `store_tests` has 34
sites and measured 36). **Do not present +660 as a call-site count, or ~530 as a directory count.**

---

## 5. Corrected historical-count reconciliation

### 5.1 The arithmetic error

Batch 3 listed remaining deltas `+43, +31, +25, +21, +21, +18, +14, +10` and stated the total as
**+157**. The sum is **+183**. I mis-summed and then derived a "27% of remaining leakage" figure from
the wrong denominator (43/183 = 23.5%). The Batch 3 report file was left unedited to preserve the
audit trail; Batch 4 corrected it.

### 5.2 The larger problem: the tracked inventory was incomplete

Progressive claims of remaining leakage across batches, and what the source actually shows:

| after | claimed remaining | actually remaining (verified) | gap |
|---|---:|---:|---:|
| Batch 3 | 157 (should be 183) | — | −26 arithmetic |
| Batch 4 | 183 | 183 | 0 |
| Batch 5 | 140 | 140 | 0 |
| Batch 6 | 122 | 122 | 0 |
| **this audit** | — | **660** | **+538 untracked** |

The gap did not come from arithmetic — it came from **19 fixture families that had never been
measured or inventoried at all**, plus 3 `api_tests` families that zero-argument matching reported as
empty:

| newly discovered family | file | sites | measured |
|---|---|---:|---:|
| `test_state_with_ai_router` | `api_tests.rs` | 114 | ↑ 308 |
| `test_state_with_ai_router_and_embedder` | `api_tests.rs` | 11 | ↑ 308 |
| `test_state_with_playwright_stub` | `api_tests.rs` | 3 | ↑ 308 |
| `new` | `journey_tests.rs` | 37 | 9 |
| `new` | `phase_7_1_regression_tests.rs` | 29 | 11 |
| `new` | `integration_e2e_tests.rs` | 29 | 16 |
| `new` | `phase_7_3_decision_tests.rs` | 17 | 12 |
| `new` | `pre_phase_8_tests.rs` | 17 | 12 |
| `new` | `phase_7_4_decision_tests.rs` | 14 | 10 |
| `test_state` | `process_lifetime_maps_tests.rs` | 9 | 10 |
| `test_state` | `key_scope_tests.rs` | 7 | 9 |
| `test_state` | `ws_tests.rs` | 8 | 9 |
| `test_state` | `rbac_matrix_tests.rs` | 7 | 8 |
| `setup_test_store` | `operations_tests.rs` | 15 | 15 |
| `setup_test_store` | `adaptive_intelligence_tests.rs` | 10 | 9 |
| `temp_store` | `qa_memory_planning_tests.rs` | 13 | 14 |
| `temp_store` | `qa_memory_perf_tests.rs` | 6 | 7 |
| `temp_store` | `qa_memory_validation_tests.rs` | 6 | 7 |
| `harness` | `trace_chain_tests.rs` | 10 | 12 |
| `harness` | `risk_signal_tests.rs` | 10 | 11 |
| `test_state` | `run_map_retention_tests.rs` | 4 | 6 |

### 5.3 Conclusion

**Every previous "remaining total" was a floor, and a badly low one.** The root cause is
methodological: prior batches enumerated *helpers whose names were already known*, rather than
enumerating *construction sites* and then grouping them. Two whole-tree sweeps with corrected
patterns (parameterised calls; `TempDir::new()`) found 538 directories per run that no report had
counted.

---

## 6. Fresh artifact measurements and attribution limitations

### 6.1 Method

Each suite run once, counting `%TEMP%` `.tmp*` directories immediately before and after. Per-test
attribution (running each test individually with `--exact`) was used where a suite total needed to be
split, and a sanity check confirmed the filter actually selected tests before results were trusted.

### 6.2 Results

| suite | tests | delta | suite | tests | delta |
|---|---:|---:|---|---:|---:|
| **api_tests** | 316 | **308** | `rbac_matrix_tests` | 15 | 8 |
| `store_tests` | 36 | 36 | `qa_memory_perf_tests` | 7 | 7 |
| `integration_e2e_tests` | 20 | 16 | `qa_memory_validation_tests` | 7 | 7 |
| `operations_tests` | 15 | 15 | `run_map_retention_tests` | 6 | 6 |
| `qa_memory_retrieval_tests` | 21 | 21 | `rbac_probe_tests` | 17 | 4 |
| `autopilot_store` | 17 | 21 | `risk_semantics_tests` | 10 | 3 |
| `qa_memory_risk_tests` | 21 | 19 | `qa_memory_migration_tests` | 1 | **0** |
| `risk_prioritisation_tests` | 20 | 19 | `risk_prioritisation_tests` | 20 | 19 |
| `trace_tests` | 21 | 18 | `ws_tests` | 9 | 9 |
| `change_impact_tests` | 19 | 14 | `key_scope_tests` | 9 | 9 |
| `qa_memory_planning_tests` | 14 | 14 | `journey_tests` | 13 | 9 |
| `phase_7_3_decision_tests` | 14 | 12 | `adaptive_intelligence_tests` | 39 | 9 |
| `pre_phase_8_tests` | 13 | 12 | `process_lifetime_maps_tests` | 12 | 10 |
| `trace_chain_tests` | 14 | 12 | `phase_7_4_decision_tests` | 11 | 10 |
| `phase_7_1_regression_tests` | 15 | 11 | `retention_and_metrics_tests` | 10 | 10 |
| `risk_signal_tests` | 18 | 11 | | | |

Already-fixed suites re-measured during this audit: **all 0**.

### 6.3 Attribution limitations — stated plainly

* **Suite-level deltas are not per-helper rates.** Several suites contain more than one fixture
  (e.g. `retention_and_metrics_tests` has 2 tempdir sites; `api_tests` has 6 families and 4 inline
  fixtures). For suites with a single helper the attribution is unambiguous; for `api_tests` it is
  **shared across 6 families** and is *not* split per family.
* **No per-test sweep was run for the 30 non-api suites.** Their deltas are suite-level. That is
  sufficient to rank them, and sufficient to prove they leak, but not to locate the leak to a single
  test. Each should get a per-test sweep during its own batch if a residual appears after a fix.
* **Where site count ≠ delta**, the difference is unexplained and **not guessed**: 29 sites/16 dirs,
  34 sites/36 dirs, 37 sites/9 dirs, 20 sites/21 dirs. Some of those tests share a fixture; some
  sites are in helpers that tests do not all reach. These are flagged, not resolved.
* **`autopilot_runner` and `store.rs` inline tests were not measured** — they live in production
  files and are excluded from the total.

---

## 7. Store / pool / TempDir ownership findings

### 7.1 The common shape

All 33 remaining helper families are variations on:

```rust
let dir = tempfile::tempdir()?;
let store = Store::connect(&dir.path().join("<name>.sqlite3")).await?;
(store, dir)                 // or (AppState, dir), or a service bundle
```

| object | holds the pool? |
|---|---|
| `Store` | **yes** — `pub struct Store { pub pool: SqlitePool }` |
| `TempDir` | owns the directory holding the database |
| `Arc<ExecutorRegistry>` | no — maps of `Arc<dyn Executor>` |
| `Arc<LocalEvidenceStore>` | no — `root: PathBuf` |
| `AppState` | **yes**, via `pub store: Store` |

Because `Pool` is `Clone`, every `store.clone()` handed to a service, dispatcher or executor shares
one pool, so a single close covers all of them.

### 7.2 Three distinct defect classes confirmed in the source

1. **No close at all** — the overwhelming majority (all families in §3.2 except `api_tests`).
2. **A second pool on the same file** — confirmed in three files:
   * `engine_tests.rs` `d2`/`d3` — **already fixed in Batch 5**
   * `qa_memory_tests.rs` `migrations_preserve_rows_written_before_them` — **already fixed in Batch 6**
   * **`api_tests.rs` — 7 `reloaded` stores** at L11020, 11064, 11127, 11308, 11478, 11621, 21740,
     each an independent `Store::connect` on the same `api.sqlite3`, **none closed**. Plus
     L11868/11888/11955 which open additional stores in scoped blocks.
3. **Closes that are fault injection, not cleanup** — see §7.3.

### 7.3 Existing `pool.close()` calls, classified

Classification required by the brief. **No helper is called fixed merely because it contains a close.**

| file | close count | purpose | verdict |
|---|---:|---|---|
| `engine_tests.rs` | 44 | 2 are **fault injection** ("Induce a genuine persistence failure by closing the store's connection pool"); rest are Batch 5 cleanup | **correct — preserve the 2** |
| `api_tests.rs` | 2 | **fault injection** — the doc comment states "state.store.pool.close() is a genuine read failure — the same technique engine_tests uses" | **correct — preserve both** |
| `governed_healing_tests.rs` | 50 | Batch 4 cleanup | correct |
| `qa_memory_tests.rs` | 36 | Batch 6 cleanup | correct |
| `agent_execution_tests.rs` | 22 | Batch 3 cleanup | correct |
| `exploration_tests.rs` | 25 | Batch 2 cleanup | correct |
| `integrity_tests.rs` | 34 | 9.2A cleanup | correct |
| `phase_8` / `phase_9` | 3 each | Batch 1 cleanup | correct |
| `phase_9_2a_lifecycle_tests.rs` | 6 | 9.2A cleanup | correct |
| `journey_tests.rs` | 2 | **unclassified — needs review** | ⚠ |
| `integration_e2e_tests.rs` | 4 | **unclassified — needs review** | ⚠ |
| `phase_7_1_regression_tests.rs` | 3 | **unclassified — needs review** | ⚠ |
| `qa_memory_risk_tests.rs` | 2 | **unclassified — needs review** | ⚠ |
| `risk_prioritisation_tests.rs` | 1 | **unclassified — needs review** | ⚠ |
| `phase_7_3_decision_tests.rs` | 1 | **unclassified — needs review** | ⚠ |
| `change_impact_tests.rs` | 3 | **unclassified — needs review** | ⚠ |

### 7.4 Correctly awaited cleanup already present

The 8 fixed families (§3.1) hold **0** un-awaited `pool.close();` and **0** `impl Drop`-based async
shutdown. Verified by grep in this audit.

---

## 8. Background-execution findings

The brief requires this be checked per family, not assumed. Results:

| risk class | families | evidence |
|---|---|---|
| **No background execution** | `qa_memory_retrieval`, `qa_memory_risk`, `risk_prioritisation`, `trace`, `change_impact`, `qa_memory_planning`, `trace_chain`, `risk_signal`, `process_lifetime_maps`, `key_scope`, `ws`, `rbac_matrix`, `qa_memory_perf`, `qa_memory_validation`, `run_map_retention`, `rbac_probe`, `risk_semantics`, `operations`, `adaptive_intelligence`, `phase_7_1/7_3/7_4`, `pre_phase_8` | zero `tokio::spawn`, `spawn_blocking`, `JoinHandle`, `spawn_run`, `engine::`, `axum::serve` |
| **Not yet checked** | `store_tests`, `autopilot_store`, `autopilot_runner`, `integration_e2e_tests`, `journey_tests` | flagged; must be checked in their batch |
| **No background execution, with a detached non-DB thread** | `api_tests` | see §9 |

The only *test-suite* background work found anywhere is in `api_tests`: a `std::thread::spawn` +
dedicated `tokio::runtime` that serves a `/health` route on `127.0.0.1:8080` (L8389–8415). It holds
**no `Store`** and does not touch the fixture's database. It is not a teardown risk for the pool.

---

## 9. Dedicated `api_tests` ownership map

`api_tests` is the single largest remaining family: **311 fixture call sites**, measured **+308
directories per run**, ~47% of all remaining leakage.

### 9.1 The six fixture families

| helper | line | signature | sites | returns |
|---|---|---|---:|---|
| `test_state` | L134 | `()` | 116 | `(AppState, TempDir)` |
| `test_state_with_compiled_api` | L555 | `()` | 66 | `(AppState, TempDir)` |
| `test_state_with_ai_router` | L590 | `(Arc<AiInfrastructure>)` | 114 | `(AppState, TempDir)` |
| `test_state_with_ai_router_and_embedder` | L600 | `(Arc<AiInfrastructure>, Arc<dyn Embedder>)` | 11 | `(AppState, TempDir)` |
| `test_state_with_marker_executor` | L7132 | `()` | 1 | `(AppState, TempDir)` |
| `test_state_with_playwright_stub` | L18311 | `(…)` | 3 | `(AppState, TempDir)` |

Plus **4 inline** tests that build their own tempdir and open additional stores:
`approval_survives_a_process_restart_…` (L11923), `rejection_survives_a_process_restart_…` (L12006),
`phase7c_a_run_that_was_going_when_the_service_stopped_…` (L17096),
`phase7d_a_run_at_a_gate_survives_a_restart_…` (L18209).

All six share the same body shape: `tempfile::tempdir()` → `Store::connect(api.sqlite3)` →
`LocalEvidenceStore::new` → `ExecutorRegistry` → `AppState::new(...)` → return `(AppState, dir)`.

### 9.2 `AppState` ownership

`AppState` (defined in `state.rs`) holds:

| field | pool relevance |
|---|---|
| `pub store: Store` | **holds the pool — and is `pub`, so teardown is reachable** |
| `pub registry: Arc<ExecutorRegistry>` | no pool |
| `pub evidence: Arc<LocalEvidenceStore>` | no pool |
| `pub policy: Arc<PolicyConfig>` | no pool |
| `pub runs: Arc<RwLock<HashMap<RunId, Arc<RunRecord>>>>` | **holds live `RunRecord`s; each may be the subject of a detached run** |
| `ai_infra`, `embedder`, per-project streams, … | no pool |

**Teardown is expressible without new abstractions:** `state.store.pool.close().await` reaches the
pool through a public field, exactly as the two fault-injection tests already do.

### 9.3 Findings that make `api_tests` safer than feared — and two that do not

**Good news (source-verified):**

* `tokio::spawn` in `api_tests.rs`: **0**. `spawn_run`: **0**. `.dispatch(`: **0**.
  `spawn_maintenance`: **0**.
* The two `engine::` hits are **false positives**: L8862 is `base64::engine::general_purpose`; L11956
  is a **synchronous** `engine::recover_incomplete_runs(&store)`.
* The single `axum::serve` + `TcpListener::bind` is the `/health` target described in §8 — no store.
* Tests **own** the `TempDir` (it is returned alongside `AppState`), so teardown order is under test
  control.

**Real complications:**

1. **7 second pools on the same file** (§7.2). A `state.store`-only teardown misses them.
2. **2 fault-injection closes that must not be removed** (§7.3).
3. **`AppState.runs` holds `Arc<RunRecord>`.** Even though no `spawn_run` is called from `api_tests`
   today, a `RunRecord` dropped after the pool closes is harmless, and no current test depends on a
   detached task. This is safe **today**; it is the thing to re-check if an api test ever starts a run.

### 9.4 Proposed fixture abstraction for `api_tests` (not implemented — recommendation only)

Because 311 sites span 6 helpers and 4 inline fixtures, and because 7 of them hold a *second* pool,
a bulk `state.store.pool.close().await` append would be correct for the simple families but **wrong for
the 4 restart/crash tests and for the 7 reload sites**.

Recommended shape:

```rust
/// Closes every pool this test opened on `api.sqlite3`, in reverse creation order.
/// Fault-injection closes in the calling test are intentionally NOT undone.
async fn teardown_api(state: &AppState, extra_pools: &[&Store]) { … }
```

or, simpler and lower-risk: a per-test explicit close, with the reload tests closing their
`reloaded` handle themselves. **This should be decided in the api batch, not assumed here.**

---

## 10. Recommended implementation batches

Ordered by *measured leak × fix confidence*. Per-family background risk must be re-verified in its own
batch; rows marked "not checked" in §8 are ordered later for that reason.

### Batch 7 — the simple `temp_store` family (highest confidence, 4 files)

| family | sites | measured | bg risk | 2nd pool | feasibility |
|---|---:|---:|---|---|---|
| `qa_memory_retrieval_tests::temp_store` | 20 | 21 | none found | no | trivial; sibling of Batch 6 |
| `autopilot_store::temp_store` | 20 | 21 | **not checked** | no | easy after check |
| `qa_memory_risk_tests::temp_store` | 20 | 19 | none found | no | trivial |
| `risk_prioritisation_tests::temp_store` | 20 | 19 | none found | no | trivial |
| **subtotal** | **80** | **80** | | | |

### Batch 8 — the `temp_store` family, second wave

| family | sites | measured | bg risk |
|---|---:|---:|---|
| `store_tests::temp_store` | 34 | 36 | **not checked** |
| `trace_tests::temp_store` | 18 | 18 | none found |
| `change_impact_tests::temp_store` | 16 | 14 | none found |
| `qa_memory_planning_tests::temp_store` | 13 | 14 | none found |
| `qa_memory_perf_tests::temp_store` | 6 | 7 | none found |
| `qa_memory_validation_tests::temp_store` | 6 | 7 | none found |
| `subtotal** | **93** | **96** | |

### Batch 9 — the `AppState::test_state` family outside api_tests

| family | sites | measured |
|---|---:|---:|
| `process_lifetime_maps_tests::test_state` | 9 | 10 |
| `key_scope_tests::test_state` | 7 | 9 |
| `ws_tests::test_state` | 8 | 9 |
| `rbac_matrix_tests::test_state` | 7 | 8 |
| `run_map_retention_tests::test_state` | 4 | 6 |
| `subtotal** | **35** | **42** |

Same ownership shape as `api_tests` but far smaller and with no second pools — a good rehearsal.

### Batch 10 — the `harness` family

`trace_chain_tests` (10, 12), `risk_signal_tests` (10, 11), `risk_semantics_tests` (2, 3),
`rbac_probe_tests` (3, 4) — **16 sites, 30 measured.**

### Batch 11 — the decision-phase `new()` families

`phase_7_1_regression_tests` (29, 11), `phase_7_3_decision_tests` (17, 12),
`pre_phase_8_tests` (17, 12), `phase_7_4_decision_tests` (14, 10) — **77 sites, 45 measured.**
Note the large site/delta mismatch; per-test sweeps will be needed.

### Batch 12 — integration & support

`integration_e2e_tests` (29, 16), `journey_tests` (37, 9), `operations_tests` (15, 15),
`adaptive_intelligence_tests` (10, 9), `retention_and_metrics_tests` (inline, 10).
**91 sites, 59 measured.** Background risk not checked for the first two.

### Batch 13 — `api_tests`, dedicated

**311 sites, 308 measured.** Requires its own ownership-aware plan per §9.4, must preserve the 2
fault-injection closes, and must close the 7 reload pools. **Must be last**, so the final
full-suite artifact check runs against a repository with no known leak left.

### Unassigned — ownership unresolved

`autopilot_runner::new` (18 sites, production file), `autopilot_test_support::fixture` (pub(crate)).
Neither is measured. **Needs a scoping decision before any batch.**

---

## 11. Risks and unresolved blockers

| # | risk / blocker | detail |
|---|---|---|
| 1 | **`api_tests` second pools** | 7 `reloaded` stores on one file. A `state.store`-only teardown is insufficient. Needs the §9.4 abstraction or per-test handling. |
| 2 | **Fault-injection closes** | 2 in `api_tests`, 2 in `engine_tests`. Any bulk "remove redundant closes" or "normalise teardown" pass would silently destroy the tests' subject. |
| 3 | **Unclassified existing closes** | 6 files carry `pool.close()` whose purpose is not yet classified (§7.3 ⚠). Must be classified before editing those files. |
| 4 | **Background risk unverified** | `store_tests`, `autopilot_store`, `autopilot_runner`, `integration_e2e_tests`, `journey_tests`. |
| 5 | **`autopilot_test_support::fixture`** | `pub(crate)` shared helper; no call sites found and no measured delta. Ownership unclear. |
| 6 | **Site/delta mismatches** | 5 suites where measured delta ≠ site count, unexplained (§6.3). |
| 7 | **Inventory was wrong twice** | Two pattern defects (§2.2) each hid real families. **A corrected sweep must be re-run if any further family is renamed or added.** |
| 8 | **Disk** | 53.99 GB free; each full run of the still-leaking suites costs real space. Avoid repeated full runs. |
| 9 | **No production `Store::close()`** | still unauthorized. Every fix so far is per-call-site, so the ~530 remaining sites are ~530 edits. |

---

## 12. Definitive list: fixed vs remaining

### 12.1 FIXED — 8 families, 193 call sites, all measured delta 0

`integrity_tests::harness` (28) · `phase_8_quality_tests::Db` (15) ·
`phase_9_readiness_tests::Db` (16) · `exploration_tests::harness` (19) ·
`agent_execution_tests::setup_harness` (15) · `governed_healing_tests::setup_healing_harness` (43) ·
`engine_tests::temp_store` (33) · `qa_memory_tests::temp_store` (24) ·
plus `qa_memory_tests::migrations_preserve_rows_written_before_them` (inline, 2 pools)

### 12.2 REMAINING — 33 helper functions across 30 files, ~530 call sites, measured +660

**`api_tests` alone is 311 sites / +308 — 47% of the remaining leakage.** Every other family is ≤36.

### 12.3 Measured NOT leaking

`qa_memory_migration_tests` (1 test, delta 0).

---

## 13. Acceptance criteria for closing Phase 9.2

Phase 9.2 must not be marked complete until **all** of the following hold. Each is drawn from a
failure mode actually observed during this work, not from a generic checklist.

1. **Every fixture family in §12.2 is either fixed or shown source-proven not to leak** — including the
   currently-unmeasured `autopilot_runner` and `autopilot_test_support`.
2. **The inventory is re-verified by a corrected whole-tree sweep** matching §2.1's pattern set
   (including parameterised calls and `TempDir::new()`), showing no new families. Two earlier sweeps
   were wrong; the closing sweep must be the third.
3. **Intentional fault-injection closes are intact and still exercised** — the 2 in `api_tests`, the 2
   in `engine_tests`, verified by those tests still passing for their original reason.
4. **Required background writes are not lost during teardown** — for every family that starts
   background work, a test asserts the persisted rows *before* the close, and a mutation removing the
   coordination must fail that test.
5. **No fixture fix silently changes test semantics** — zero lines removed other than documented,
   intended edits; zero assertions weakened, skipped or suppressed.
6. **Every fix is proven by mutation**, not by a green exit code: removing the awaited close must
   return the leak, with the mutation verified as applied (sha changed, correct line count, fresh
   `Compiling`, binary newer than source).
7. **Per-test attribution, not just suite totals**, for any suite where a fix does not reach zero.
8. **The full backend suite completes successfully and produces no new successful-run artifacts
   attributable to any completed fixture** — measured as a `.tmp` delta of 0 across at least two
   consecutive full runs.
9. **`cargo fmt --check` and Clippy pass** under existing policy, with `auth.rs:317` documented as
   pre-existing rather than silently fixed.
10. **All earlier uncommitted work is intact** — verified by multiset comparison against a pre-batch
    snapshot for every file touched, with any difference explained as intentional.
11. **No production code changed** across all of Phase 9.2.

---

## 14. Exact next recommended action

**Do not start a fix batch yet — Batch 6's inventory was materially wrong twice, and the cost of that
was 538 untracked directories per run.** The cheapest next action is one more narrow scan, then Batch 7.

1. **Classify the 6 unclassified `pool.close()` call sites** (§7.3 ⚠) in `journey_tests`,
   `integration_e2e_tests`, `phase_7_1_regression_tests`, `qa_memory_risk_tests`,
   `risk_prioritisation_tests`, `phase_7_3_decision_tests`, `change_impact_tests` — cleanup or fault
   injection. This is read-only and takes minutes; doing it after a fix would risk deleting a
   fault-injection close.
2. **Decide the `autopilot_test_support::fixture` and `autopilot_runner::new` ownership question**, so
   no family is left permanently unassigned.
3. **Check background-work reachability** for `store_tests`, `autopilot_store`, `integration_e2e_tests`,
   `journey_tests` — four of them are still marked "not checked" and two are large.

Then **Batch 7** (`qa_memory_retrieval_tests`, `autopilot_store`, `qa_memory_risk_tests`,
`risk_prioritisation_tests` — 80 sites, ~80 measured), whose ownership shape is identical to
`qa_memory_tests` fixed in Batch 6.

`api_tests` stays in **Batch 13**, last, because the closing full-suite artifact check must run
against a repository with no known leak left, and because its 7 second pools and 2 fault-injection
closes make it the one family that cannot be fixed by appending a single line per site.

---

*Prepared by audit only. No source file, test, migration or route was modified. No artifact directory
was deleted. Nothing was committed or pushed.*