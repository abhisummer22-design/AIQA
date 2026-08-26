# AIQA — Phase 1 Final Verification & Closure Gate

**Document:** `AIQA_PHASE1_FINAL_VERIFICATION_REPORT.md`
**Date:** 2026-08-17
**Scope:** Independent verification of Phase 1 (B1–B6, B14) in `services/orchestrator`
**Verification mode:** Adversarial. The Phase 1 implementation report was treated as a *claim under test*, not as evidence.
**Phase 2 work performed:** None.
**Source files modified during verification:** None. (`git status` after verification is byte-identical to before; the only writes were to a scratch database and this report.)

---

## 1. Scope & Method

Seven Phase 1 items were verified: **B1** (durable persistence + failure escalation), **B2** (recovery state derivation), **B3** (intelligence tenant isolation), **B4** (permit before `mark_started`), **B5** (executor timeout), **B6** (transactional run insert), **B14** (bug-draft project isolation).

Four independent evidence channels were used, in this order of authority:

1. **Direct SQLite inspection** — reading the database file with an external `sqlite3` connection, bypassing the orchestrator entirely. This is the only channel that can distinguish *durable* state from *in-memory* state, and it is the channel that produced every defect in this report.
2. **Live HTTP probes** against a running orchestrator binary.
3. **Fault injection** — inducing real `SQLITE_BUSY` by holding an `EXCLUSIVE` lock from a second process past the configured `busy_timeout`.
4. **Source inspection** and `cargo test`.

Channel 1 is emphasised deliberately. Every one of the five defects below is invisible through the HTTP API and visible only in the database, because the API serves finished runs from memory while the process lives. An API-only verification would have returned a clean PASS.

**Method note on a near-miss.** Two reproduction attempts failed before the third succeeded: setting the DB file read-only (`attrib +R`) did not disturb an already-open WAL connection, and locking before run submission tested the wrong code path. Only locking *mid-run* exercised the escalation path. Both failed attempts are reported because a verifier who stopped at attempt one would have wrongly concluded the failure path was unreachable.

---

## 2. Verification Environment

| Item | Value |
|---|---|
| Binary | `services/orchestrator/target/debug/aiqa-orchestrator.exe` |
| Database | `infra/data/orchestrator.sqlite3` (real file, not `:memory:`) |
| Restarts performed | 4 (orch3 → orch4 → orch5 → orch6) |
| Production DB | Snapshotted before verification, restored after |
| Task timeout | Default 600 s; one restart at `AIQA_TASK_TIMEOUT_SECS=2` to test B5 |

**Disclosure on environment fidelity:** the restored snapshot already contained 9 probe runs when it was taken — these originate from the Phase 1 *implementation* validation, not from this session. The database was restored to exactly the snapshot state; it was not further cleaned, and no attempt was made to present it as pristine.

---

## 3. Build & Test Suite

| Check | Result |
|---|---|
| `cargo check --all-targets` | Clean, no warnings |
| `cargo test` | **24 passed, 0 failed, 0 ignored** |
| Reproducibility | Re-run at end of verification: identical, 24/24 in 2.22 s |

The suite passes. Section 14 records what it does **not** cover.

---

## 4. B1 — Durable Persistence & Failure Escalation ❌ **PARTIAL**

### 4.1 Connection configuration — PASS

`store.rs` sets `journal_mode(Wal)`, `synchronous(Normal)`, `busy_timeout(5s)`, `foreign_keys(true)`. Verified not by reading the code but by connecting externally: `PRAGMA journal_mode` returned `wal`, and `-wal` / `-shm` sidecar files were present on disk.

### 4.2 Escalation path — reachable and correct at run level — PASS

An `EXCLUSIVE` lock was held from a second process for 20 s (4× the `busy_timeout`) while a 30-task run executed. Result:

```
run b8d284bc… : 30/30 tasks executed successfully
API state     : FAILED          ← correct, not COMPLETED
policy_notes  : "durable persistence failed during this run; recorded state
                 may be incomplete — first failure: set_task_status(final, t5):
                 error returned from database: (code: 5) database is locked"
```

Orchestrator log, 4 matching lines:

```
ERROR run b8d284bc…: durable write failed — set_task_status(final, t5): … database is locked
ERROR run b8d284bc…: durable write failed — insert_event(95b59e5a…): … database is locked
ERROR run b8d284bc…: durable write failed — insert_event(46756934…): … database is locked
ERROR run b8d284bc… marked FAILED: durable persistence failed during this run …
```

This is the designed behaviour and it works: a run whose bookkeeping was lost is **not** reported as successful. Confirmed live, not inferred.

### 4.3 Defect D2 — the failure reason is not durable — **FAIL**

The API returned `policy_notes`. The database did not:

```
API  after failure   : policy_notes = "durable persistence failed …"
DB   after failure   : policy_notes = NULL
DB   after restart   : policy_notes = NULL          ← reason permanently lost
DB   after restart   : state        = FAILED        ← state survives
```

**Root cause.** `engine.rs:524` writes the note to `record.policy_notes`, an in-memory `RwLock`. The only durable writer is `set_run_finished`:

```rust
pub async fn set_run_finished(&self, run_id: RunId, state: RunState) -> Result<()> {
    sqlx::query("UPDATE runs SET state = ?, finished_at = ? WHERE id = ?")
```

`policy_notes` is written to the database in exactly two places — `INSERT INTO runs` (`store.rs:118`) and the reject path (`store.rs:216`) — and neither runs here.

**Consequence.** The operator restarts and sees a run marked FAILED whose 30 tasks all passed, with no recorded explanation. This is precisely the case where the diagnostic matters most, and it is the one case where it is discarded. The comment at `engine.rs:513` states the note is "recorded in policy_notes … making it auditable rather than a log entry nobody reads" — after a restart it is neither.

### 4.4 Defect D3 — no reconciliation of state orphaned by the failure — **FAIL**

The same run left permanent inconsistencies in the database:

| Artefact | State after failure | After restart |
|---|---|---|
| Task `t5` | `RUNNING` | **still `RUNNING`** — never reconciled |
| Event sequences | 60 rows, seq 0–61 | **still missing seq 11, 12** |
| API event count | 62 (from memory) | 60 (from disk) |

Recovery does not touch the run because it is already terminal (`FAILED`, `finished_at` set), so the orphaned `RUNNING` task persists indefinitely. The event-sequence gap is permanent, which means `GET /runs/{id}/events?since=10` will silently skip two events forever — the `?since=` replay contract (SRD §5/§18) assumes contiguity.

The `policy_notes` text explicitly anticipates this ("recorded state may be incomplete"), so the incompleteness is *by design* — but D2 means the warning label itself does not survive, leaving corrupt data with no marker.

### 4.5 Defect D1 — two persistence sites still swallow errors — **FAIL**

```rust
// engine.rs:231
if let Err(e) = store.approve_run(record.run_id, actor).await {
    tracing::error!("failed to persist approval for run {}: {e}", record.run_id);
}   // ← execution continues; no note_persistence_failure

// engine.rs:244
if let Err(e) = store.reject_run(record.run_id, actor, reason).await {
    tracing::error!("failed to persist rejection for run {}: {e}", record.run_id);
}   // ← execution continues; no note_persistence_failure
```

**This directly contradicts the Phase 1 implementation report**, which stated that all 8 swallowed sites route through `note_persistence_failure` and marked the DoD item *"No persistence failure logged-and-ignored; every write error propagates or degrades the run explicitly"* as ✅.

The security consequence is material: a governance approval that fails to persist leaves the run executing with in-memory authorisation that no longer exists on disk. After a restart the run appears never to have been approved, while its side effects have already occurred.

---

## 5. B2 — Recovery State Derivation ✅ **PASS**

Four interrupted runs were seeded directly into the database in `RUNNING` state with known task outcomes, then the orchestrator was restarted:

| Scenario | Task statuses | Recovered | Expected | Result |
|---|---|---|---|---|
| All passed *(the original B2 defect)* | 5× PASSED | `COMPLETED` | `COMPLETED` | ✅ |
| One failed | 4× PASSED, 1× FAILED | `FAILED` | `FAILED` | ✅ |
| One infra error | 4× PASSED, 1× INFRA_ERROR | `PARTIAL` | `PARTIAL` | ✅ |
| One blocked | 4× PASSED, 1× BLOCKED | `PARTIAL` | `PARTIAL` | ✅ |

The original defect — 50/50 tasks PASSED in the database reported as run state `FAILED` — is fixed and confirmed against its exact reproduction. **B2 passes its stated Definition of Done in full.**

---

## 6. Defect D4 — Live finalisation diverges from recovery *(new finding, outside B2's scope)*

`derive_run_state` is used **only** on the recovery path (`engine.rs:301`). Live finalisation (`engine.rs:499–506`) retains the original inline logic:

```rust
if engine.tasks.values().any(|t| t.status == TaskStatus::InfraError) {
    RunState::Partial
} else {
    RunState::Completed          // ← FAILED, BLOCKED, TIMEOUT all land here
}
```

Measured live, three times:

| Run contents | Live state | Recovery would derive | Diverges |
|---|---|---|---|
| 6 passed, 3 **failed** | `COMPLETED` | `FAILED` | Yes |
| 1 failed, 1 **blocked** | `COMPLETED` | `PARTIAL` | Yes |
| 2 passed, 1 **timeout** | `COMPLETED` | `PARTIAL` | Yes |

Identical task outcomes therefore produce different run states depending on whether the process happened to restart. Independently of the divergence, a QA orchestrator reporting `COMPLETED` for a run containing failed and timed-out tests is misleading on its own terms.

**Classification:** this is *not* a B2 failure. B2's planned scope was explicitly the recovery path ("B2 — Recovery marks fully-passed runs FAILED"), and calling this a Phase 1 regression would mean inventing an acceptance criterion that was never written. It is a **pre-existing inconsistency newly exposed** by B2's introduction of a second, correct derivation function, and it belongs in the Phase 2 backlog.

Note that §12's restart test masked this: that run contained infra errors, so both code paths returned `PARTIAL` and agreed by coincidence.

---

## 7. B3 — Intelligence Tenant Isolation ✅ **PASS**

`task_history_by_name(name, project_filter, limit)` joins `runs` and applies `COALESCE(r.project_id,'default') = ?`. `intelligence.rs` passes `Some(project_id)`.

Live, two projects running an identically-named task:

| Caller | Window | Failure rate | Correct |
|---|---|---|---|
| Project A | 3/3 | 1.0 | ✅ own data only |
| Project B | 0/1 | 0.0 | ✅ no contamination from A |

The original defect (project B reporting `failure_rate=0.75` from A's three failures) is gone.

---

## 8. B4 — Permit Before `mark_started` ✅ **PASS**

`try_acquire_owned()` is taken before `mark_started` and before the `TASK_STARTED` emit; failure `break`s the dispatch loop.

Live, 24 tasks at `concurrency=3`, polled throughout execution:

```
concurrency limit    = 3
observed peak RUNNING = 3        → bound respected, never exceeded
TASK_STARTED events   = 24 / 24  → no double-start, no phantom start
final state           = COMPLETED
```

---

## 9. B5 — Executor Timeout ✅ **PASS**

Restarted with `AIQA_TASK_TIMEOUT_SECS=2`; a task requesting 15 s of work:

```
elapsed        2.1s          ← cancelled, not waited out
slow  → TIMEOUT  "executor exceeded the 2s task timeout and was cancelled"
fast  → PASSED
after → PASSED               ← permit was released; later task still ran
```

`TIMEOUT` is reachable (it was previously dead), cancellation is real rather than a status relabel, and the permit is not leaked. `kill_on_drop(true)` is set on all four command builders.

The run-level `COMPLETED` visible here is D4, not a B5 defect.

---

## 10. B6 — Transactional Run Insert ✅ **PASS**

Insert is wrapped in `pool.begin()` / `commit()` with `QueryBuilder::push_values` batching at `CHUNK = 200`.

**Atomicity under real failure.** A run submitted while an `EXCLUSIVE` lock was held returned HTTP 500, and the database was then inspected directly:

```
runs named 'lock-contention-probe' persisted : 0   → atomic, no partial row
orphaned tasks with no parent run            : 0   → no dangling children
```

The failure is surfaced to the caller rather than half-written — the correct outcome.

**Scale.** Six run sizes to 1000 tasks, executed end-to-end (not insert-only):

| Tasks | Insert | Execute | Events | Expected 2n+2 | Task count |
|---|---|---|---|---|---|
| 10 | 0.01 s | 1.01 s | 22 | 22 ✅ | exact |
| 50 | 0.01 s | 1.4 s | 102 | 102 ✅ | exact |
| 100 | 0.02 s | 1.8 s | 202 | 202 ✅ | exact |
| 250 | 0.03 s | 2.6 s | 502 | 502 ✅ | exact |
| 500 | 0.04 s | 4.2 s | 1002 | 1002 ✅ | exact |
| **1000** | **0.07 s** | **7.31 s** | **2002** | **2002 ✅** | exact |

Zero persistence errors logged across all six.

---

## 11. B14 — Bug-Draft Project Isolation ✅ **PASS**

Scoping is enforced server-side: `list_bug_drafts(project_filter)`, and `review_bug_draft` resolves ownership via `bug_draft_project(id)` before `require_project_access`.

| Probe | Result |
|---|---|
| A lists drafts | 1 (own) ✅ |
| B lists drafts | 1 (own) ✅ |
| Admin lists drafts | 2 ✅ |
| **B reviews A's draft** | **403** ✅ |
| A reviews own draft | 204 ✅ |
| B reads A's run | 403 ✅ |
| B reads A's evidence | 403 ✅ |

Both halves of the original finding — cross-tenant **read** and cross-tenant **mutation** — are closed.

---

## 12. Restart & Recovery Integrity (clean shutdown) ✅ **PASS**

456-task run (400 passed / 25 failed / 15 infra / 15 blocked / 1 real pytest), compared field-by-field across a restart:

| Property | Before | After | Match |
|---|---|---|---|
| Run state | `PARTIAL` | `PARTIAL` | ✅ |
| Task count | 456 | 456 | ✅ |
| Event count | 899 | 899 | ✅ |
| Sequence range | 0–898 | 0–898 | ✅ |
| Sequence gaps | 0 | 0 | ✅ |
| Event ID set | 899 | 899 identical | ✅ |
| Evidence refs / IDs / sizes | 1 / `6308caf7…` / [970] | identical | ✅ |
| Per-status counts | B15 F26 I15 P400 | identical | ✅ |
| Stuck `RUNNING`/`QUEUED` | 0 | 0 | ✅ |
| Per-task drift | — | 0 | ✅ |

Artifact remained retrievable after restart (968 chars). **On a clean shutdown, durability is exact.** The failures in §4.3–4.4 arise only when a write *fails* mid-run.

---

## 13. Regression Suite ✅ **PASS (8/8)**

| Behaviour | Result |
|---|---|
| Continue-on-failure | ✅ 8 independents completed despite t2/t7 failing |
| Dependency blocking | ✅ dependents correctly `BLOCKED` |
| Realtime events | ✅ 23 events, seq 0–22, 0 gaps |
| `?since=` reconnect | ✅ correct suffix |
| Policy gate | ✅ `PENDING_APPROVAL` |
| Approval flow | ✅ HTTP 200 |
| Executor registry | ✅ real pytest → `FAILED` |
| Evidence capture | ✅ 1 artifact, 970 B |

No Phase 1 change regressed existing behaviour.

---

## 14. Discrepancies Against the Phase 1 Implementation Report

| # | Report claim | Verified reality |
|---|---|---|
| 1 | "All 8 swallowed persistence sites now route through `note_persistence_failure`" | **False.** `engine.rs:232` and `engine.rs:245` still log and continue (D1). |
| 2 | DoD *"No persistence failure logged-and-ignored"* marked ✅ | **Not met**, per above. |
| 3 | "Cause is recorded in `policy_notes`, making it auditable" | **True in memory only.** Never written to disk; lost on restart (D2). |
| 4 | Phase 1 presented as fully verified | The escalation path had **zero** automated coverage: `grep` for `persistence_error\|note_persistence_failure` across `store_tests.rs` and `engine_tests.rs` returns **0 matches**. It was verified for the first time in this session, by fault injection. `approve_run`/`reject_run` likewise have **0** test references. |

The 24 passing tests are genuine and do prove their targets. They simply do not reach the code paths where the defects live — which is why the suite is green while D1–D3 are real.

---

## 15. Defects, DoD Matrix, Remediation & Final Status

### 15.1 Defects

| ID | Sev | Defect | Location | Proof |
|---|---|---|---|---|
| **D1** | **HIGH** | Approval/rejection persistence failures logged and ignored | `engine.rs:232`, `engine.rs:245` | Source |
| **D2** | **HIGH** | Persistence-failure reason never written to disk; lost on restart | `engine.rs:524` + `store.rs` `set_run_finished` | Live: DB `policy_notes = NULL` |
| **D3** | MED | Task orphaned in `RUNNING`; permanent event-sequence gaps; no reconciliation | recovery path | Live: `t5` RUNNING, seq 11–12 missing |
| **D4** | MED | Live finalisation ignores `derive_run_state`; FAILED/BLOCKED/TIMEOUT runs report `COMPLETED` | `engine.rs:499–506` | Live ×3 |
| **D5** | MED | No test coverage for escalation path or approve/reject persistence | `engine_tests.rs`, `store_tests.rs` | `grep` → 0 |

### 15.2 Definition of Done

| # | DoD item | Status |
|---|---|---|
| 1 | WAL + busy_timeout active | ✅ PASS |
| 2 | No persistence failure logged-and-ignored | ❌ **FAIL** (D1) |
| 3 | Persistence failure degrades the run explicitly | ⚠️ PARTIAL — run degrades ✅, cause not durable ❌ (D2) |
| 4 | Recovery derives state from task statuses | ✅ PASS (4/4) |
| 5 | Intelligence scoped by project | ✅ PASS |
| 6 | Permit acquired before `mark_started` | ✅ PASS |
| 7 | Executor timeout enforced, `TIMEOUT` reachable | ✅ PASS |
| 8 | Run insert transactional and batched | ✅ PASS |
| 9 | Bug drafts isolated (read + mutate) | ✅ PASS |
| 10 | Restart integrity | ✅ PASS clean / ❌ FAIL post-write-failure (D3) |
| 11 | Tests prove the old defect | ⚠️ PARTIAL (D5) |

**9 of 11 pass. Two fail, both in B1.** B2–B6 and B14 pass without qualification.

### 15.3 Remediation

All four code fixes are small and localised; none touches the API contract, database schema, or frontend.

1. **D1** — route both sites through the existing helper:
   ```rust
   if let Err(e) = store.approve_run(record.run_id, actor).await {
       record.note_persistence_failure("approve_run", e).await;
   }
   ```
   Same for `reject_run`. ~4 lines.
2. **D2** — extend `set_run_finished` to accept and persist the note (`UPDATE runs SET state = ?, finished_at = ?, policy_notes = COALESCE(?, policy_notes) WHERE id = ?`). ~6 lines.
3. **D3** — on startup, reconcile tasks left `RUNNING` under a terminal run to `INFRA_ERROR` with a recorded reason. ~15 lines.
4. **D4** — call `derive_run_state` at `engine.rs:499` so live and recovery share one derivation. ~5 lines. *Flag for Phase 2 sign-off:* this changes externally visible run states (a run with failed tests becomes `FAILED`, not `COMPLETED`) and so needs a contract decision before it ships.
5. **D5** — add two tests: (a) close the `SqlitePool` mid-run and assert the run degrades to `FAILED` with a durable cause; (b) assert an approval whose write fails does not leave the run silently executing.

### 15.4 Final Status

> # PHASE 1 — PARTIAL

Six of seven items (**B2, B3, B4, B5, B6, B14**) pass independent verification against their stated Definitions of Done, several against exact reproductions of the original defects. Build is clean, 24/24 tests pass, 1000-task scale is exact, cross-tenant isolation is closed, and clean-shutdown durability is byte-exact across 899 events.

**B1 is incomplete.** Its headline behaviour works — a run whose writes failed is correctly marked `FAILED` rather than reported successful, proven by live fault injection. But two persistence sites still swallow errors (D1), directly contradicting the implementation report and failing DoD item 2 outright; and the recorded cause never reaches disk (D2), so the one artefact an operator needs after a restart is the one thing discarded.

Per the closure-gate rule — *if any critical persistence requirement fails, mark PARTIAL/FAIL* — the verdict cannot be PASS.

> # CAN PHASE 2 START: **NO**

**Reasoning.** Phase 2 is the API foundation: `/api/v1` prefix, project scoping, migrations 0004/0005/0007, structured error envelope. It builds *directly on top of* the persistence and approval layer that D1 and D2 leave unsound. Specifically:

- Phase 2 formalises the governance/approval surface in the contract. Shipping it over an approval path that can lose its write without anyone noticing (D1) would bake a silent authorisation gap into the public API.
- Phase 2 adds migrations. Reconciling orphaned `RUNNING` tasks and event-sequence gaps (D3) is far cheaper before new schema layers sit on top of the corrupt rows.
- D2 and D3 degrade *diagnosability*. Debugging Phase 2 integration failures against a store that discards its own corruption markers will cost more than the fix.

**This is not a large blocker.** D1 and D2 together are roughly ten lines plus two tests. The recommendation is to clear **D1, D2, and D5** — the items that fail a stated Phase 1 DoD — re-run this verification, and start Phase 2 on a PASS. **D3** should be scheduled with the Phase 2 migrations. **D4** requires a contract decision on run-state semantics and should be resolved during Phase 2 planning rather than patched silently, since it changes what the API reports.
