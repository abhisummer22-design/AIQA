# Phase 6 — PASS

**Phase 6 of `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`: Data Integrity + Cross-Layer
Intelligence.** Full evidence in
[`PHASE_6_DATA_INTEGRITY_REPORT.md`](./PHASE_6_DATA_INTEGRITY_REPORT.md).

Predecessor: [`PHASE_5_PASS.md`](./PHASE_5_PASS.md) — Phases 1–5 not restarted or rewritten.

---

## Result

**PASS.**

| Criterion | Status | Where proven |
|---|---|---|
| real cross-layer evidence can be collected | PASS | 11 comparisons over 5 layers, every one fed by a pre-existing reader and a real production writer; 25 tests |
| entity identity is reliable | PASS | 5 stored-key sources; ambiguous → `INSUFFICIENT_DATA` with candidates named |
| consistent state is recognized correctly | PASS | 5 tests for agreement, across status, event log, duration, fingerprint and run state |
| genuine mismatches are detected | PASS | 8 comparison kinds each detect their own mismatch |
| insufficient data is handled honestly | PASS | reached by 7 distinct routes; `data_sufficiency` reported; every non-defect carries a caveat |
| database verification remains read-only | PASS | zero `sqlx`, zero SQL, zero `&mut self` in the module; runtime sentinel unchanged |
| project isolation passes | PASS | all six cross-run comparisons empty for another tenant; identical fingerprints do not leak |
| tests pass | PASS | 25/25 |
| regression passes | PASS | 1934 / 0 failed / 5 ignored |
| no duplicate evidence/execution/memory subsystem | PASS | no new table, no new engine, no new memory |

---

## The audit finding

**No cross-layer comparison exists — but every layer has a real writer, and every reader
already existed.** `tasks` ← `set_task_status`, `execution_telemetry` ← `ingest_run`,
`runs`/`failure_clusters`/`evidence` ← their own writers, `audit_log`/`run_events` ← `insert_audit`
/ `insert_event`, and the UI layer ← `CrawlOutput`.

Phase 6 needed **one** new store reader and **no migration**. The migration count is unchanged at
32.

---

## Read-only turned out to be structural

An audit for arbitrary-query or read-only-query infrastructure found **none**: no `query_only`,
no `exec_sql`, no query builder anywhere. `Store` is a hand-written reader per table with fixed
text and bound parameters.

That is a stronger answer to the brief than a policy would have been:

* there is **no query surface to constrain**, so arbitrary SQL cannot be introduced by forgetting
  a rule;
* "reuse the existing governed read mechanism" resolves to "call the existing readers";
* `IntegrityVerifier` holds a `&Store`, so it is structurally incapable of writing.

Verified mechanically rather than asserted: `integrity.rs` contains **zero** `sqlx`, **zero** SQL
write verbs in its code, and **zero** `&mut self` methods — the only occurrence of that string is
in the doc comment stating there is none. `the_verifier_cannot_write` proves it three ways: a
comment-stripped source guard, and a runtime sentinel whose content and four table row counts are
unchanged after all eleven comparisons run.

---

## Two findings that matter more than the feature

**F2 — Phase 4's report is wrong about `security_sensitivity`, and this corrects it.**
Phase 4 stated the column had "no column and no table anywhere". It **does** exist, and
`intelligence.rs:553` writes it as the literal `0.0` — along with `business_criticality` and
`change_frequency`. The risk formula weights it at **10%**, so every stored risk score is silently
understated by a fabricated zero.

The correction changes the remediation: Phase 4's report implies a **migration** is needed. The
truth is the column is already there and a **signal source** is missing — a writer to add, not a
schema to change. Phase 4's report is left unmodified, because rewriting a past report to match a
later finding would be fabricating what was known when.

**F1 — `set_task_status` silently drops `finished_at`.** Its branches are
`if set_started { … } else if set_finished { … }`, so one call with both flags takes only the first
branch. **Latent, not active**: `engine.rs` always makes two calls. Reported, not fixed — it is
production scheduling code and changing its write semantics is out of scope.

---

## F2 became a standing guard

`ComparisonKind::FabricatedConstant` and `fabricated_constant_check` exist because of F2. A single
row cannot distinguish "measured, came out nil" from "written as a constant" — but across *every*
row, a column that never varies carries no information, and that **is** detectable.

`the_fabricated_constant_check_finds_the_hardcoded_zeros` proves it catches all three hardcoded
columns, flags them `is_measurement: false`, and returns
`defect_established: false` with the caveat *"a finding about AIQA's own scoring, not about the
application under test; no bug record should be raised against the application from it."*

If a future change wires a real signal source into one of those columns, the column varies, the
check stops reporting it, and nothing else changes.

---

## `INCONSISTENT` is not a defect — and the module says so

These are different claims, so they are separate fields. `defect_established` requires **both**
observations to be *measurements*: a constant, a default, or a value with no signal source cannot
establish anything about the application.

**Two of eleven comparisons can reach it today** — task status and duration — because those are
the pairs written by genuinely independent code at genuinely different moments. The other nine are
one-sided absence, bookkeeping, or pairs whose timestamps cannot be shown to align. Every one of
those carries a `defect_caveat` saying so.

---

## Staleness is handled before disagreement

The window (60 s, stated as a constant and asserted) is checked **first**, so a differing value
too far apart to compare becomes `INSUFFICIENT_DATA` citing staleness — never `INCONSISTENT`.

Proved with an A/B on identical data: the default window yields `INCONSISTENT`; a zero window
yields `INSUFFICIENT_DATA`. Two real writers never stamp the same instant, so a zero window
exercises the rule without backdating a row by hand.

---

## Bugs found

1. **A bug in my own comparison logic, caught by a test.** The first fingerprint comparison
   hardcoded the API side as `false` against the database's `true`, reporting `INCONSISTENT` for
   *every* fingerprint regardless of the data. Fixed by recognising the two layers hold
   *different propositions* — "a failure occurred" vs "a cluster exists" — which must not be
   compared for equality.
2. **A test asserted a state the system cannot reach.** It required a fingerprint with no
   registered cluster; `ingest_run` writes both together. Rather than fabricate the state, the
   test now asserts the real coupling and the defensive branch is documented as such.
3. **My harness called `set_task_status(true, true)`** — which F1 shows drops `finished_at`. The
   two duration tests failed with *"the backend recorded no start/finish pair"*, which is the
   module correctly reporting a missing measurement rather than a mismatch.
4. **The read-only source guard tripped on the module's own doc comment**, which names `&mut self`
   to state there is none. Now strips comments first.
5. **A vacuous assertion** (`u64 >= 0`) that clippy escalated to an error, replaced with two that
   actually test something.

Nothing was weakened, skipped or deleted to pass. In (2) the fix was to **stop asserting something
untrue**.

---

## Performance — measured, not pinned

| Tasks | Verify | Per task |
|---|---|---|
| 5 | 6 ms | 1.20 ms |
| 20 | 8 ms | 0.40 ms |
| 60 | 20 ms | 0.33 ms |

Database read (85 rows): **2 ms**. Evidence (50 writes): **26 ms**.

**Per-task cost falls as the entity set grows**, so the work is dominated by a few fixed reads —
and the measurement asserts that rather than assuming it. No N+1 pattern: each comparison reads
its tables once and answers every entity from that read, the same discipline Phase 4 established
after finding 2,000+ redundant reads there.

---

## Limitations carried forward

1. **The UI layer has no per-element timestamp**, so `API_UI` can never establish a defect.
2. **The fingerprint/cluster comparison is weak by construction** — one call writes both layers.
3. **Only two of eleven comparisons can establish a defect.** §4 of the report states which, and
   why.
4. **Requirement, coverage and memory layers are not compared** — not among the five the brief
   names, and no `covers` edge exists to join on.
5. **Audit comparison covers only targets that parse as a run id.**
6. **F1 reported, not fixed** — the footgun remains.
7. **F2 reported, not fixed** — the columns are still `0.0`; the guard now detects it.
8. **Trace linkage is by `test_case_id` and only when supplied**; nothing forces a bug record.
9. **The live database is unchanged** — no figure here is a production claim.
10. **No E2E CI job** — pre-existing.

---

## Discipline

* No test weakened, skipped or deleted. Zero regressions.
* No migration. One new reader, fixed text, one bound parameter — named columns, so it cannot
  become a query builder.
* No second evidence system, database abstraction, assertion engine, execution engine or memory
  system.
* The most common answer is `INSUFFICIENT_DATA`, and that is correct: a verifier that answered
  every question would be inventing answers.
* A prior phase's report was corrected by addition, never by rewriting it.
* Two production defects were found; both are reported with their blast radius assessed, and
  neither was "fixed" by changing behaviour outside this phase's scope.

---

**Phase 7 not started. Nothing committed. Nothing pushed.**
