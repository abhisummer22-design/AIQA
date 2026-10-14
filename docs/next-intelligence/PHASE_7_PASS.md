# Phase 7 — PASS

**Phase 7 of `AIQA_NEXT_ROADMAP_PHASED_PLAN.md`: Production-like User Journey Intelligence.**

> **Roadmap objective (L201):** "Test complete business journeys rather than only isolated cases,
> while reusing existing browser/API executors and Action AST."
>
> **Roadmap PASS gate (L239):** "A journey must be executable through existing governed paths,
> produce step-level evidence, report a coherent final verdict, and pass isolation and regression
> tests."
>
> **Roadmap deliverables (L243–244):** `PHASE_7_USER_JOURNEYS_REPORT.md`, `PHASE_7_PASS.md`.

Predecessor: [`PHASE_6_PASS.md`](./PHASE_6_PASS.md) — Phases 1–6 not restarted or rewritten.

This file is the second roadmap deliverable. It records **no new implementation**. Every claim below
is quoted or derived from the five Phase 7 reports named in the header, all of which predate it.
Phase 7.3 recorded that creating this file "is a documentation step for the phase close, not an
implementation claim" ([7.3 §16](./PHASE_7_3_DECISION_IMPLEMENTATION_REPORT.md)).

---

## Evidence base

| report | verdict it records |
|---|---|
| [`PHASE_7_USER_JOURNEYS_REPORT.md`](./PHASE_7_USER_JOURNEYS_REPORT.md) | **PHASE 7 = BLOCKED** (§13) |
| [`PHASE_7_1_REMEDIATION_REPORT.md`](./PHASE_7_1_REMEDIATION_REPORT.md) | **PHASE 7.1 = PASS** (§15); *"PHASE 7 remains BLOCKED"* (§16) |
| [`PHASE_7_2_REMAINING_GAPS_AUDIT.md`](./PHASE_7_2_REMAINING_GAPS_AUDIT.md) | audit; no implementation |
| [`PHASE_7_3_DECISION_IMPLEMENTATION_REPORT.md`](./PHASE_7_3_DECISION_IMPLEMENTATION_REPORT.md) | **PHASE 7.3 = PASS**; **PHASE 7 = PASS** (§16) |
| [`PHASE_7_4_RISK_METRIC_IMPLEMENTATION_REPORT.md`](./PHASE_7_4_RISK_METRIC_IMPLEMENTATION_REPORT.md) | **PHASE 7.4 = PASS**; *"Phase 7 stands at PASS"* (§18) |

**Phase 7 did not pass on its first submission.** Its own report wrote, at L306:
`docs/next-intelligence/PHASE_7_PASS.md` is **deliberately not created** — the PASS gate is not met.
The gate was met three sub-phases later. Phase 7.4 recorded the outstanding deliverable and the
standing verdict together:

> "Also outstanding, unrelated to the next phase: `docs/next-intelligence/PHASE_7_PASS.md` — the
> roadmap deliverable closing Phase 7. … **Phase 7 stands at PASS** under the documented
> supported-target scope (Q3: `requirement` / `test_case` / `failure_cluster`)." — 7.4 §18

---

## Result

**PASS.**

| Criterion | Status | Where proven |
|---|---|---|
| journey executable through existing governed paths | PASS | all six journeys A–F through real writers; 13 tests |
| step-level evidence produced | PASS | telemetry, fingerprints, clusters, evidence objects |
| coherent final verdict | PASS | per-run integrity verdicts; §8 notes the absence of a journey-level aggregate as a limitation, not a gate breach |
| isolation and regression tests pass | PASS | 13 journeys ×10; `journey_tests` 20/0; `integrity_tests` 20/0 |
| all discovered bugs fixed and retested | PASS | P7-D1 and P7-D2 fixed in 7.1, retested, mutation-verified |
| every required roadmap test covered | PASS | 12 of 12, per the main report §4 and §5; two later reclassified — see below |
| full suite green at close | PASS | 2057 passed / 0 failed / 5 ignored |
| formatting / Clippy | PASS | 1 pre-existing diff; clippy exit 0 |

---

## How the gate was actually met

Phase 7's first report closed **BLOCKED** on two production defects it found and was not authorised
to fix:

* **B1** — the integrity verifier masked **16 storage reads**: a failed read was reported as a clean
  run.
* **B2** — a blocked step was published with a **0% failure rate**.

Both fixes needed production changes outside Phase 7's authority (a public API change across 34 call
sites; a data-semantics decision affecting an existing contract). 7.1 §16 records that fixing them was
still not enough:

> "Its two reported blockers, B1 and B2, are **resolved**. Phase 7 is blocked for a different reason…
> **A `module`-target change has no provable impact.** `change_impact.rs::resolution_note("module")`
> still refuses: *'a module vertex exists, but no `covers` edge is ever written'*."

Under the roadmap gate — *"a journey must be executable through existing governed paths"* — a
workflow a user can reach that produces nothing is not validated. That, not B1 or B2, was the
remaining blocker.

**7.2** audited that gap and raised seven numbered decisions (Q1–Q7) without assuming any answer.
**7.3** implemented the approved answers, and **Q3 answered the module question by scope**:

> "The roadmap's required test list is fully exercised, and the one open question — whether
> module-change support is mandatory — is **now answered by approved scope (Q3)** rather than left
> ambiguous. Under the documented supported-target set, every mandatory journey validates… No journey
> remains blocked by an unresolved contract." — 7.3 §16

---

## Decisions implemented (7.3 §16)

| # | Decision | Result recorded |
|---|---|---|
| Q1 | metric contract for S2–S11/S15 — **B3** | 7 sites carry both rates; **no existing value changed** |
| Q2 | `INFRA_ERROR` treatment | excluded from both halves of the completed rate; counted and reported |
| Q3 | supported set | the three approved kinds — `requirement` / `test_case` / `failure_cluster` |
| Q4 | module support | **deferred**, prerequisites recorded in §7 |
| Q5 | accept-list / resolution contradiction | one authoritative `SUPPORTED_CHANGE_KINDS`, three call sites derived |
| Q6 | `set_task_status` signature | documented, **not redesigned**; enum explicitly deferred |
| Q7 | refused-change persistence | `audit_log` `denied`, **zero** `change_impacts` rows |

7.4 then made the completed-execution rate the risk factor's input across all five in-scope
consumers, with `INFRA_ERROR` excluded from product completed-failure calculations.

---

## Verified test results

### The 12 required roadmap tests

The main report's §4 maps all twelve (roadmap L224–235). At that point:

| Required test | Where | Then | Now |
|---|---|---|---|
| complete successful journey | A | PASS | PASS |
| failure at an intermediate step | B (step 3 of 3) | PASS | PASS |
| state not preserved between steps | J10 | PASS | PASS |
| invalid prerequisite order | J8, J9 | PASS (after a correction, §6) | PASS |
| duplicate step/action | J11 | PASS | PASS |
| retry/resume safety | J11 | PASS | PASS |
| role/authorization boundary | J5, J13 | PASS | PASS |
| project isolation | J14, F | PASS | PASS |
| unsupported or missing step evidence | C (module), J10 | module **BLOCKED** | **PASS under Q3** |
| cancellation and timeout | — | **NOT APPLICABLE** | **NOT APPLICABLE** |
| evidence and trace links | A, E | PASS | PASS |
| journey selection after a supported change | C | PASS | PASS |

Two honesty notes carried forward rather than smoothed over:

* **"cancellation and timeout" was never satisfied and never could be.** There is no journey cancel
  path; the DAG's timeout is per-task (`AgentTaskPlan.timeout_secs`), validated by `PlanValidator`.
  Recorded as NOT APPLICABLE, not simulated (main §12.4).
* **The module row was BLOCKED at the time and was closed by a scope decision (Q3), not by writing a
  `covers` edge.** No `covers` edge is written by this work. Module-change support remains deferred
  (7.4 §15).

### Full backend suite progression

| point | passed / failed / ignored | delta |
|---|---|---:|
| baseline before Phase 7 | 2004 / 0 / 5 | — |
| Phase 7 main | **2017 / 0 / 5** | +13 (the 13 journeys) |
| Phase 7.1 | **2032 / 0 / 5** | +15 (new regression module) |
| Phase 7.2 (audit) | 2032 / 0 / 5 | 0 — no code change |
| Phase 7.3 | **2046 / 0 / 5** | +14 |
| Phase 7.4 — **Phase 7 close** | **2057 / 0 / 5** | +11 |

Every delta is stated in its own report as exactly the tests added. No pre-existing test was removed,
skipped or weakened at any point.

### Regression at close

| check | result |
|---|---|
| 13 journeys ×10 | 10/10 each — 130 assertions, 0 failures (7.4 §17) |
| `journey_tests` ×20 | 20 passed / 0 failed |
| `integrity_tests` ×20 (previously-flaky duration tests) | 20 passed / 0 failed |
| `phase_7_1_regression_tests` ×20 | 20 / 0 |
| `phase_7_3_decision_tests` ×20 | 20 / 0 |
| full backend suite | **2057 / 0 / 5** |
| `cargo fmt --check` | 1 diff — `auth.rs:317`, pre-existing at HEAD, untouched |
| `cargo clippy --all-targets` | exit 0 |

Per-file lints: 0 in every changed file, including each new test module (7.1 §12, 7.3 §14, 7.4 §17).

---

## Mutation evidence

* **7.1** ran 6 mutations. M1/M1d/M5 were **not caught** and that was recorded as a coverage
  limitation — whole-pool failure injection makes only the *first* read in a chain observable.
  M6 caught a defect introduced during the fix itself.
* **7.3** ran 4 mutations: 4 caught, and **1 failed attempt was discarded** rather than counted.
* **7.4** gave each of the five in-scope consumers a test that fails if its input is reverted, and
  restored every mutation **SHA-byte-identical**.

Every report states the mutation was verified applied before its result was read.

---

## Limitations carried forward

Recorded, not resolved. Numbers as at Phase 7 close.

1. **No journey-level verdict aggregate.** Only per-run integrity verdicts exist (main §12.4;
   7.1 §14.4).
2. **Cancellation and timeout are NOT APPLICABLE** — no journey cancel path exists.
3. **No live executor or browser was driven**, so journey-shaped *application* behaviour is
   unvalidated. Everything about persistence, comparison and reporting is.
4. **Module-change support (Q4) remains deferred** pending an authoritative module-identity source;
   lifting it may need a migration (7.4 §15).
5. **`code_file` / `api_contract` / `page` targets remain unresolvable** (main §12.6).
6. **D3 — canonical risk formula: OPEN.** No scoring function was merged; three of six factors
   (0.30 of the model) are structurally unmeasured (7.4 §15).
7. **D5 — duration tolerance: OPEN.** The 10 ms floor is unchanged.
8. **D6 — `set_task_status` API: OPEN.** No enum, no redesign.
9. **`failure_severity` still sees `INFRA_ERROR`** — `api.rs:9243` raises severity to `0.8` when one
   is present. Left as-is and reported, because Decision 1 authorised changing the *failure-rate*
   factor's input only. Should be decided together with D3 (7.4 §15).
10. **`completed_failure_rate` scale inconsistency** — the API response is 0..1, the QA-memory view
    structs are 0..100. Unifying is a breaking response change and needs its own authorisation
    (7.4 §18).
11. **Integrity verifier per-read attribution is unverified** — only the first read in a chain is
    observable under whole-pool failure injection (7.1 §14.1).
12. **The API handler's error mapping is not mutation-verified**, because the auth read fails first
    (7.1 §14.2).
13. **Single-node SQLite only.**

---

## One unresolved inconsistency in the source

`PHASE_7_1_REMEDIATION_REPORT.md` is internally inconsistent about its own final count. Its §11 table
and its downstream users say **2032 / 0 / 5** (before 2017, delta +15), while §8 and §15 say
**2031 / 0 / 5** with a delta of +14.

The report itself explains the origin at §8: a restore left a stale build artifact, cargo did not
rebuild, and the run reported `FAILED. 2031 passed; 1 failed` — 2031 passed **plus one failure**,
i.e. 2032 total. After forcing a rebuild, "the same tests passed and the suite read **2032 / 0 / 5**."

The 2031 figure at §8 and §15 was therefore a leftover from the discarded stale-binary run and was
not corrected in those two places. **2032 is the operative number**: it is what 7.2 (§1) and 7.3
(§14) both carry forward as their baseline. This is recorded rather than silently reconciled — the
source report is left unmodified.

---

## Discipline

* No test weakened, skipped or deleted at any point in Phase 7. Every delta in the suite table is
  stated by its own report as exactly the tests added.
* **No migration** and no new route. No risk-formula weight or threshold introduced — all six weights
  and all four thresholds asserted unchanged (7.4 §17).
* No second DAG, regression runner, evidence store, or execution engine.
* Where a required test could not be honestly satisfied, it was recorded as NOT APPLICABLE or
  BLOCKED rather than simulated. Two of the twelve required tests carry that history permanently.
* Mutations that were not caught, and one mutation attempt that failed, were **discarded and
  reported**, not counted as coverage.
* An environmental failure (C: drive at 0 MB, 897 failures) was diagnosed by contrast rather than
  assumed, and the `TEMP` redirect used to obtain clean numbers was removed afterwards. 7.3 records:
  "No repository file, migration or config was changed to achieve this."
* The `auth.rs:317` formatting diff was left untouched in every phase.

---

## Repository state

Nothing committed. Nothing pushed. HEAD `7930507`, 0 ahead / 0 behind `origin/main` at the time all
five reports were written.

Phases 1–6 not restarted or rewritten. The original `PHASE_7_USER_JOURNEYS_REPORT.md` is kept
verbatim as the audit record; its §13 `BLOCKED` verdict stands as written and is superseded only as
to the status of B1 and B2 (7.1 §16).

**Phase 9 has since been started; Phase 9.2 remains open. Phase 10 has not been started.**
