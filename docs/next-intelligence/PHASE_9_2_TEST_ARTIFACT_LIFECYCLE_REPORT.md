# PHASE 9.2 — TEST-ARTIFACT LIFECYCLE REPORT

**Verdict: BLOCKED.** Not because the defect is unclear — it is now measured precisely — but
because **the fix that works requires a scope decision that is yours, not mine**, and because the
root cause recorded in the Phase 9.1 audit is **empirically disproven**.

Repository: `https://github.com/abhisummer22-design/AIQA.git`
Branch: `main` · HEAD: `7930507` · 0 ahead / 0 behind
Audit under review: `PHASE_9_1_CONSUMER_AND_GAPS_AUDIT.md`

---

## 1. The headline: the audit's root cause is wrong

Phase 9.1 §14 concluded:

> *"Rust drops locals in reverse declaration order — so `_dir` drops **first**, attempting
> `remove_dir_all` while `state` — and its open SQLite pool — is still alive."*

and recommended `impl Drop for Store { pool.close() }` as a ~5-line fix.

**Both the cause and the fix are wrong.** I measured it. Five fixtures per case, on this OS:

| case | what it does | leaked |
|---|---|---|
| **A** | current fixture order (`state` first, `_dir` second) | **5 of 5** |
| **B** | **corrected** order (`_dir` first, `state` second) | **5 of 5** |
| **C** | `pool.close().await`, then drop | **0 of 5** |

**Correcting the drop order changes nothing.** Dropping the last `Pool` handle does *not* release
the SQLite file on Windows. Only an **awaited** `Pool::close()` does.

The Phase 9.1 audit drew its conclusion from reading source rather than running it. It also
recommended `impl Drop for Store { pool.close() }`, which — per the API check in §2 — **would not
have compiled usefully at all**.

---

## 2. The API check the brief demanded

`sqlx-core-0.9.0`, `src/pool/mod.rs:441`:

```rust
pub fn close(&self) -> impl Future<Output = ()> + '_ { self.0.close() }
```

* It is **not** `async fn` — it returns `impl Future`. `impl Drop for Store { pool.close() }` would
  construct a future and immediately drop it un-awaited, doing nothing while appearing to work.
* Its own docs (L436–438): *"If the returned Future is not run to completion, any remaining
  connections will be dropped when the last handle … is dropped."* Empirically, on Windows with
  SQLite, that is **not sufficient** to release the file.

I also verified `spawn_maintenance_tasks` holds `Arc::downgrade` (L508–510), so background tasks do
not pin the pool alive — yet dropping still leaks. So the connection release is deferred somewhere
`Drop` cannot reach synchronously. `Pool::close()` is the only reliable release path.

---

## 3. State verification (Step 1)

| | measured | reported | match |
|---|---|---|---|
| HEAD / upstream | `7930507`, 0/0 | same | ✅ |
| modified / untracked / deleted | 25 / 24 / 0 | 25 / 23 (+ this report) | ✅ |
| leftover artifacts | **6,984 dirs / 8.00 GB** | 6,984 / 8.01 GB | ✅ |

**No existing artifact was deleted**, per the safety rule. The probe itself leaked 11 directories
while running; those remain.

Verified from source: 39 candidate fixture helpers; ~700 destructuring sites dominated by
`let (state, _dir)`, `let (store, _d)`, `let (store, _dir)`; **33 existing `pool.close()` calls
already in tests** — the correct pattern is already established in this codebase.

---

## 4. Why I stopped instead of implementing

Three constraints in the brief collide with the measured reality:

1. *"Do not attempt to await inside a normal Drop implementation."* — `Drop` cannot await.
2. *"Do not modify production `Store` semantics unless investigation proves that a test-only
   fixture solution cannot reliably fix the actual defect."*
3. *"If a production `Store` change appears necessary, stop and report the justification before
   expanding scope."*

A test-only fix is possible but **cannot be automatic**: because drop-order is irrelevant and only
an awaited close works, every fixture must have that await *executed*. Rust has no async `Drop`, so
the await must appear at each of the ~700 call sites, or inside a helper the test explicitly drives.

That is a change to ~700 sites of test code — far beyond "adjust helper return order and fixture
destructuring", and large enough to risk altering test behaviour, which the brief forbids me from
doing casually. **That is a decision requiring your approval, not mine to take unilaterally.**

---

## 5. Options (all verified against the measurement above)

| option | change | blast radius | my assessment |
|---|---|---|---|
| **A** | Add an async teardown owner type; migrate the 39 helpers; add `await` teardown at ~700 sites | very large, test-only | Works, but is a major test diff. Should be its own phase |
| **B** | Add a production `Store::close(self) -> impl Future`; helpers call it in teardown | large, production + tests | Smallest code, but changes production semantics — needs your authorisation |
| **C** | Targeted: migrate only the highest-volume helpers (`api_tests`, `qa_memory_*`, `engine`) | moderate, test-only | Gets most of the benefit; the rest still leaks |
| **D** | Do nothing; document and manage TEMP externally | none | Rejected — the brief rules out a cleanup script, and the leak is real |

**Recommendation: C**, then A for the remainder, sequenced deliberately — **or** B if you prefer
one authoritative mechanism over a test-only one. I did not choose for you because the brief
explicitly reserves this.

---

## 6. What I did not do

* No production code changed. `store.rs` is untouched.
* No test behaviour changed. No existing test was edited, weakened, skipped or deleted.
* No migration, no route, no threshold, no formula, no commit, no push.
* No artifact deleted.
* The probe files (`p92_probe.rs`, `store_fixture_probe.rs`) were **temporary** and are removed;
  `lib.rs` is byte-restored to its pre-probe state.

Post-cleanup verification: `cargo build --tests` 0 errors; full suite **2101 passed / 0 failed /
5 ignored**; working tree back to 25 modified / 24 untracked / 0 deleted.

---

## 7. Verification results obtained

| check | result |
|---|---|
| `cargo build --tests` | 0 errors |
| Full backend suite | **2101 passed / 0 failed / 5 ignored** |
| Drop-order measurement (5 fixtures × 3 cases) | A 5/5 leaked · B 5/5 leaked · C 0/5 leaked |
| `sqlx-core` API read | `pub fn close(&self) -> impl Future<Output = ()> + '_` |
| Existing correct pattern in repo | 33 `pool.close()` awaits already present |

---

## 8. Remaining limitations

1. **The defect is not fixed.** ~700 fixtures still leak. A full suite run still adds artifacts.
2. **My Phase 9.1 audit §14 is wrong and should be corrected.** Its `impl Drop for Store` proposal
   would have been a no-op. Its drop-order theory is disproven by measurement.
3. **The connection-release mechanism is unexplained.** `Pool::close()` demonstrably works and
   plain drop demonstrably does not; why is not established. Worth knowing before choosing a fix.
4. **No claim is made about the historical full-suite failure.** The disk mechanism is real and now
   quantified, but causation for that specific unnamed failure remains unestablished.
5. **6,984 historical directories remain**, per instructions.

---

## 9. Decisions required

| # | decision |
|---|---|
| **D-1** | Choose the option in §5 (A, B or C). |
| **D-2** | If B: authorise a production `Store::close()`, which Phase 9.2's brief said to stop and report first. |
| **D-3** | Approve a correction to `PHASE_9_1_CONSUMER_AND_GAPS_AUDIT.md` §14. |
| **D-4** | Confirm whether the ~700-site migration is acceptable as its own phase. |

---

## 10. Final verdict

**BLOCKED** — on a scope decision, with a correct and measured diagnosis.

Not blocked on understanding: the defect is confirmed, quantified, and the only effective remedy is
identified. Blocked because every remaining path requires either a production change or a very large
test change, and the brief reserves both for your approval.

**No claim of PASS is made.** The artifact leak is unfixed, and I will not report a pass for a phase
whose stated purpose was to stop it.