# PHASE 7 — Production-like User Journeys

**Phase:** PHASE 7 — PRODUCTION-LIKE USER JOURNEY INTELLIGENCE
**Verdict:** **BLOCKED** — all six journeys validated, but two genuine production defects were
found, and fixing either requires a production API change this phase is not authorised to make.

**Production code changed: none.** One new test module, one `mod` line, and one report.

---

## 1. Initial repository state and baseline

Verified at the start of this phase, not carried forward.

| | |
|---|---|
| Branch | `main` |
| HEAD | `7930507` — *fix: correct risk signal measurement handling* |
| Upstream | `origin/main`, 0 ahead / 0 behind |
| Modified tracked files | 22 (all Phase 6.1 + F2 work, already reported) |
| Untracked files at start | 6 |

### Baseline re-measured in this run

| Check | Result |
|---|---|
| **Full backend suite** | **2004 passed / 0 failed / 5 ignored** |
| `integration_e2e_tests` | 20 passed / 0 failed |
| `integrity_tests` | 34 passed / 0 failed / 1 ignored |
| Compile errors | 0 |
| `cargo fmt --check` | 1 diff — `auth.rs:317`, pre-existing at HEAD |
| `cargo clippy --all-targets` | exit 0 |

No user change was modified or discarded. Nothing was committed or pushed.

---

## 2. Exact Phase 7 roadmap scope

From `AIQA_NEXT_ROADMAP_PHASED_PLAN.md` §PHASE 7.

**Objective:** *"Test complete business journeys rather than only isolated cases, while reusing
existing browser/API executors and Action AST."*

**Ten required tests:** complete successful journey · failure at an intermediate step · state not
preserved between steps · invalid prerequisite order · duplicate step/action · retry/resume safety ·
role/authorization boundary · project isolation · unsupported or missing step evidence ·
cancellation and timeout · evidence and trace links · journey selection after a supported change.

**PASS gate:** *"A journey must be executable through existing governed paths, produce step-level
evidence, report a coherent final verdict, and pass isolation and regression tests."*

### The architectural finding that shapes the phase

**AIQA has no journey entity.** No `journeys` table, no journey struct, no journey route. A search
for `journey` across `services/orchestrator/src` returns **4 hits**: three in the new test module and
one line of prose in `skill_files.rs`. No migration creates one; `store.rs` has no journey writer or
reader.

So the roadmap's capabilities resolve into two groups:

| Roadmap capability | Status | Composed from |
|---|---|---|
| 1. ordered steps with prerequisites + state assertions | satisfied | `TaskDef.prerequisites` DAG, `PlanValidator::validate` |
| 2. reuse existing cases, executors, evidence | satisfied | `insert_test_case`, `SimulatedExecutor`, `LocalEvidenceStore` |
| 3. verify state preservation between steps | satisfied | two-flag `set_task_status`; each step carries its own clock |
| 4. attach requirement and trace links | satisfied | `link_case_to_requirement` → `validates` edge |
| 5. identify the failing step and its evidence | satisfied | telemetry + `failure_fingerprint` + cluster |
| 6. safe resume/replay where idempotency is understood | **partial** | run is keyed by task id, so a re-record updates rather than duplicates (J11) |
| 7. report journey verdict separately from task verdicts | **partial** | run-level integrity verdict exists; no journey-level aggregate |
| 8. select journeys affected by a change-impact record | satisfied | `change_impacts` → `Changed` band |
| 9. preserve role boundaries | satisfied | operator/viewer enforcement (J5, J13) |
| 10. avoid duplicating an existing DAG or regression runner | honoured | reused both; nothing new built |

I did **not** invent a journey abstraction. That would be feature development, and this phase is
validation.

---

## 3. Journey maps and expected outcomes

| Journey | Entry point | Writer | Reader / consumer | Expected outcome | Status |
|---|---|---|---|---|---|
| **A** Document → coverage | `POST /test-cases` + document ingest | `parse_document` → `upsert_requirement_document` → `insert_test_case` → `link_case_to_requirement` | `list_test_cases_for_requirement`, `GET /requirement-impact`, `requirement_history` | covered requirements resolve to the stored case **id**; an uncovered one stays honestly empty | **PASS** |
| **B** Execution → failure intelligence | run/task creation, `ingest_run` | `insert_run_for_release`, two-flag `set_task_status`, `from_task_state`, `record_failure` | telemetry, cluster, `bug_history`, safety floor, integrity | the failing step alone produces a fingerprint, a cluster and a promoted draft | **PASS** |
| **C** Change → regression selection | `POST /change-impact` | `ChangeImpactReader::analyse` → `insert_change_impact` | `prioritise_tests`, `not_selected` | linked step is `Changed`; untouched steps carry a reason | **PASS** (module target **BLOCKED**) |
| **D** Risk → testing decision | persisted journey evidence | — (derived) | `test_risk`, `prioritise_tests` | measured and unmeasured factors both reported; the recommendation is backed by real rows | **PASS** |
| **E** Failure investigation → planning | repeated failure + evidence | `insert_evidence`, automatic draft promotion | investigation/timeline/findings endpoints, `generate_planning_feedback` | evidence retrievable; cluster relations served; planning receives the real signal | **PASS** |
| **F** End-to-end integrity | both projects' data | as B | `list_execution_telemetry`, `IntegrityVerifier`, QA Memory | isolation holds; stale evidence never becomes a defect | **PASS** (closed-store sub-path **FAIL → P7-D1**) |

### Test setup and database

- Fresh migrated SQLite per scenario via `Store::connect` → `sqlx::migrate!("./migrations")`, all 32
  migrations. No schema is hand-built.
- No external service, credential or dataset. `SimulatedExecutor` and an inert AI router
  (`FakeExecutor` returning a fixed body) so `AppState` can be built; **no network call is made**.
- API keys generated in-process by `auth::generate_key`; only hashes stored. **No secret appears in
  this report.**
- **No journey claims a live browser or external executor ran.** Steps are driven through the
  simulate spec and the production ingestion writer, and each journey says so.

---

## 4. The 12 required roadmap tests → coverage

| Required test | Where | Status |
|---|---|---|
| complete successful journey | A | PASS |
| failure at an intermediate step | B (step 3 of 3) | PASS |
| state not preserved between steps | J10 (blocked dependent) | PASS |
| invalid prerequisite order | J8, J9 | PASS — see the correction in §6 |
| duplicate step/action | J11 | PASS |
| retry/resume safety | J11 (re-recording one step id) | PASS |
| role/authorization boundary | J5, J13 | PASS |
| project isolation | J14, F | PASS |
| unsupported or missing step evidence | C (module), J10 | module **BLOCKED**, rest PASS |
| cancellation and timeout | — | **NOT APPLICABLE** — see §7 |
| evidence and trace links | A (trace), E (evidence) | PASS |
| journey selection after a supported change | C | PASS |

---

## 5. Journey-by-journey results

| Test | Result |
|---|---|
| `journey_a_document_to_test_coverage` | PASS — 4 requirements parsed; 3 covered cases resolve to stored ids; FR-003 honestly empty; HTTP-created case returns from `requirement-impact` under the same id |
| `journey_b_execution_to_failure_intelligence` | PASS — 3 steps, 3 telemetry rows; only the failing step carries a fingerprint; one cluster, `occurrence_count >= 2`; exactly one auto-generated draft; QA Memory `2 executions / 2 failed / 1 fingerprint`; integrity compares all three steps by `run_task_key` |
| `journey_c_change_to_regression_selection` | PASS — impact rows persisted; linked step selected; untouched step excluded with a reason; `Changed` band derived from the persisted row. Module target **BLOCKED** and asserted as such |
| `journey_d_risk_to_testing_decision` | PASS — clean step `failure_rate == Some(0.0)` citing rows; alternating step measured flakiness; unsourced factor still `None`; no calibration on any step; failing step is `Mandatory` and outranks the clean step |
| `journey_e_failure_investigation_and_planning` | PASS — evidence retrievable; cluster reached; draft `NEEDS_REVIEW` before review; review endpoint returns **204** and moves it to `CONFIRMED`; a viewer is refused; planning receives a `recurring_defect` signal carrying the real occurrence count |
| `journey_f_end_to_end_data_integrity` | PASS — same fingerprint, two clusters, zero cross-reads; D5 boundaries inclusive/exclusive; 48 h skew → `InsufficientData` with 0 defects. Closed-store sub-path **FAIL → P7-D1** |
| `a_prerequisite_naming_an_unknown_step_is_rejected` | PASS |
| `a_forward_prerequisite_is_legal_because_a_dag_has_no_declaration_order` | PASS (a correction, §6) |
| `a_prerequisite_cycle_is_refused` | PASS |
| `a_failed_intermediate_step_blocks_its_dependent_and_records_why` | PASS, with **P7-D2** recorded |
| `a_duplicate_step_id_is_the_same_step_not_a_second_one` | PASS |
| `a_failed_read_mid_journey_does_not_report_the_journey_as_complete` | PASS — 4 consumers + planning all fail |
| `a_whole_journey_is_isolated_from_an_identical_journey_in_another_project` | PASS |

Stability: **each of the 13 journeys ran 10/10** individually, and the module **20/20**.

---

## 6. Reproducible failures and verified root causes

Two are **production defects**. Both were found by the journeys, not by inspection.

### P7-D1 — the integrity verifier reports a failed storage read as a clean run

**Reproduced.** Journey F, with the pool closed:

```
verify_run(...) -> Ok(IntegrityReport {
    inconsistent: 0, defects_established: 0, comparisons: 0,
    layers_observed: [], layers_unavailable: [Ui, Api, Backend, Database, Audit]
})
```

A consumer checking `defects_established == 0` reads *"this run is clean"* when nothing was verified.

**Root cause.** `integrity.rs` contains **16 masked store reads** (`unwrap_or_default()` / `.ok()`) across
`verify_run` itself and **10 comparators**: `telemetry_for_run`, `task_status_consistency`,
`task_duration_consistency`, `task_reason_consistency`, `run_state_consistency`,
`fingerprint_registration`, `event_log_consistency`, `audit_trail_consistency`,
`evidence_index_consistency`, `before_after_state`.

This is the same R1 class Phase 6.1 fixed in `qa_memory.rs`, but **it never reached this module** —
Phase 6.1's R1b summary explicitly recorded *"integrity.rs has a separate `telemetry_for_run`
(5 call sites) … not part of R1b"*.

That the two readers disagree is what makes this a defect rather than a policy: on the same closed
pool, `QaMemoryReader::test_stability` correctly returns `Err` (asserted in the same test).

**Why not fixed here.** The fix requires `verify_run` and ~10 comparators to return `Result`,
rippling through **34 `verify_run` call sites** (`integrity_tests` 23, `integration_e2e_tests` 5,
`journey_tests` 4, `api.rs` 1, plus the definition). That is a public API change and a change to the verifier's failure
mode. Phase 7 is scoped to validation, and STEP 7 directs stopping at that point. → **Blocker B1.**

### P7-D2 — a blocked journey step is published with a 0% failure rate

**Reproduced.** Journey failure-path test, a step whose prerequisite failed:

```
blocked telemetry row: status BLOCKED, end_time: None, failure_fingerprint: None   (correct)
test_stability(view): executions: 1, passed: 0, failed: 0, other_outcomes: 1,
                      failure_rate: Some(0.0)                                      (wrong)
```

A step that **never executed** is counted as one execution and given a **0% failure rate** — the
reading is "this case passes".

This contradicts a contract the same reader already honours: `failure_rate` is `None` below two
executions because *"a rate from one sample would be a claim, not a measurement"*. A blocked step is
not a passing sample.

**Root cause.** `TaskStatus::Blocked` is terminal, so `ingest_run` records it (that part is honest and
useful — the step was reached and refused). `test_stability` then counts every row for the case as an
execution without distinguishing blocked ones, and computes a rate over them.

**Why not fixed here.** Whether a `BLOCKED` step belongs in a case's *execution history* is a data
semantics decision with existing contracts behind it, and it would change the `executions` contract
that Phase 6.1 and 6.2 tests assert. That is a product call, not a validation fix. → **Blocker B2.**

### Corrections to my own test assumptions

Recorded because each was wrong before it was corrected.

| I assumed | Reality |
|---|---|
| a forward prerequisite (step declared before the one it depends on) is invalid | **It is legal and accepted.** A prerequisite is a DAG edge, not a declaration-order constraint. I was wrong; both the correction and the corrected test are in the module. |
| a blocked step produces no telemetry | It does — `BLOCKED`, `end_time: None`. Honest, and better than silence. |
| `bug_history` reports only confirmed defects | It reports **every** draft and carries `review_state` on the row. |
| reviewing a finding returns 200/201 | It returns **204 No Content**. |
| a clean step has `unmeasured_weight == 0` | It is ~0.45. Unsourced factors are **structural**, not data-dependent — a project with perfect history still cannot produce a canonical score. This is the sharpest D3 finding of the phase. |

### Non-reproduction worth recording

My first change-impact mutation attempt in this phase did not fail the suite. As in Phase 6.1 I
checked whether the mutation had applied rather than accepting the green result; it had not. Re-applied
and verified — see §9.

---

## 7. Carried-forward Phase 6.2 gaps

Re-verified against current source, not assumed.

| Gap | Status | Evidence |
|---|---|---|
| 1. no module-change impact path | **STILL OPEN** | `resolution_note("module")` still refuses: *"a module vertex exists, but no `covers` edge is ever written"*. Note `store::link_test_case_to_module` **does** write a `covers` edge, so the note is now narrower than the code — it is reachable from `trace.rs` but not from any supported journey entry point. |
| 2. `code_file`/`api_contract`/`page` unresolvable | **STILL OPEN** | same table refuses all three with stated reasons |
| 3. no cluster → bug-draft promotion | **CORRECTED — this was wrong** | `intelligence.rs::record_failure` **does** auto-promote at `occurrence_count >= 2`, guarded by `has_bug_draft_for_cluster`. My Phase 6.2 report said no such path existed; I searched for the store writer and read the `intelligence.rs` call sites without following the enclosing function. Journeys B and E exercise the real promotion, including the threshold boundary. |
| 4. no live executor driven | **STILL OPEN** | no browser available; journeys use real writers/readers and never claim otherwise |

**Where Journey C stalls.** It completes for a `requirement` target. At the point a user would
declare *"the checkout module changed"*, `analyse` returns `Insufficient` with a reason naming the
missing `covers` edge, and the journey stops there — asserted, not worked around. **Downstream
impact:** a module-level change cannot produce any regression selection, so a team changing one
module gets no selection and must declare per-requirement instead.

**Journey D is affected by D3, not blocked by it.** The journey needs a *recommendation*, and
prioritisation works from bands rather than one canonical score. It does not need the single
authoritative score that D3 has not settled.

---

## 8. D3, D5, D6 — status and impact

| | Status | What Phase 7 did |
|---|---|---|
| **D3** canonical risk formula | **OPEN** | Journey D asserts the documented contracts independently: `failure_rate` measured with cited rows; `security_sensitivity` unmeasured with a basis; **`calibration: None` on all three steps**. The strongest new evidence: unmeasured weight is ~0.45 even for a step with perfect history, because three factors have no source in this schema. So *no amount of additional execution evidence can produce a canonical score* — D3 is not blocked on data. No formula chosen, merged, or altered. |
| **D5** 10 ms duration floor | **OPEN** | Journey F measures the present behaviour at every boundary: governs at 0 ms, 1 ms gap is noise, boundary inclusive at 10.0, exclusive at 10.001, relative governs above the 100 ms crossover, a 5 000 ms gap still disagrees. **Tested as implemented; not treated as a proven universal threshold; not changed.** |
| **D6** `set_task_status` signature | **OPEN** | Validated the documented two-flag contract throughout: every journey step records a start and a separate finish; a start-only call leaves `finished_at` unset (J10 asserts the blocked step carries no end time). **Not redesigned.** |

---

## 9. Discrimination — verified by mutation

A passing test is evidence of nothing until it has been seen to fail. Three production mutations:

| Mutation | Result |
|---|---|
| `test_stability` masks its telemetry read again (undoes R1b) | **caught** — `a_failed_read_mid_journey…` and `journey_f` fail |
| Automatic bug-draft promotion disabled (`occurrence_count >= 2` → `false`) | **caught** — `journey_b` and `journey_e` fail |
| Prioritisation ignores `change_impacts` | **caught** — `journey_c` fails |

All three files restored and verified **byte-identical** (SHA-256 match). No assertion was removed or
weakened to obtain a green result; six of my own wrong assertions were corrected *toward* stricter
claims about the code, and each correction is recorded in §6.

---

## 10. Regression results

| Check | Result |
|---|---|
| Each of the 13 journeys ×10 | **10/10 each** |
| `journey_tests` ×20 | **20 passed / 0 failed** |
| `integrity_tests` ×20 (previously-flaky duration tests) | **20 passed / 0 failed** |
| **Full backend suite** | **2017 passed / 0 failed / 5 ignored** |
| Baseline before this phase | 2004 / 0 / 5 — delta is exactly the 13 new tests |
| `cargo fmt --check` | 1 diff — `auth.rs:317`, pre-existing at HEAD |
| `cargo clippy --all-targets` | **exit 0** |
| Lints in the new module under `-D warnings` | **0** |

Related suites, all passing: `integration_e2e_tests` 20 · `integrity_tests` 34 (+1 ign) ·
`qa_memory_risk_tests` 21 · `qa_memory_retrieval_tests` 21 · `qa_memory_planning_tests` 14 ·
`risk_prioritisation` 20 (+1 ign) · `risk_semantics_tests` 10 · `risk_signal_tests` 18 ·
`change_impact_tests` 19 · `adaptive_intelligence_tests` 39 · `store_tests` 36 · `engine_tests` 33 ·
`api_tests` 316 · `trace_tests` 21 · `exploration_tests` 29 (+1 ign) · `governed_healing_tests` 53.

---

## 11. Files changed

| File | Nature | Justification |
|---|---|---|
| `services/orchestrator/src/journey_tests.rs` | **new**, 13 tests | the deliverable |
| `services/orchestrator/src/lib.rs` | **+5 lines** | register and document the module |
| `docs/next-intelligence/PHASE_7_USER_JOURNEYS_REPORT.md` | **new** | this report |

**Production code changed: none.** No migration, route, formula weight or threshold added or altered.
The only production file touched is `lib.rs`, and only to register the module.

`docs/next-intelligence/PHASE_7_PASS.md` is **deliberately not created** — the PASS gate is not met.

---

## 12. Remaining risks and limitations

1. **B1 — integrity verifier masks 16 storage reads.** A failed read is reported as a clean run.
2. **B2 — a blocked step is published with a 0% failure rate.**
3. No journey-level verdict aggregate exists; only per-run integrity verdicts.
4. Cancellation and timeout are **NOT APPLICABLE** — there is no journey cancel path. The DAG's
   timeout is `AgentTaskPlan.timeout_secs`, validated per task by `PlanValidator`; no journey-level
   cancellation exists to test. Recorded rather than simulated.
5. **No live executor or browser** was driven, so journey-shaped *application* behaviour is
   unvalidated. Everything about persistence, comparison and reporting is validated.
6. Module-change impact and `code_file`/`api_contract`/`page` targets remain unresolvable.
7. Single-node SQLite only; multi-tenant isolation is covered by unit tests, not here.
8. The duplicate-step guarantee comes from the run being keyed by task id; it is a property of the
   schema, not an explicit dedup rule, and would not hold for a journey whose steps share an id by
   design.

---

## 13. Final verdict

### PHASE 7 = **BLOCKED**

| Gate | Result |
|---|---|
| journey executable through existing governed paths | **Yes** — all six, through real writers |
| step-level evidence produced | **Yes** — telemetry, fingerprints, clusters, evidence objects |
| coherent final verdict | **Partial** — per-run yes; no journey-level aggregate |
| isolation and regression tests pass | **Yes** — 2017/0/5, integrity 20/20 |
| **all discovered bugs fixed and retested** | **No — P7-D1 and P7-D2 found, not fixed** |

Two reproducible production defects were discovered. Both fixes require changes Phase 7 is not
authorised to make: B1 a public API change across 34 call sites in a verification path, B2 a
data-semantics decision affecting an existing contract. Per STEP 7 and the instruction that *"a
blocked journey is a valid finding"*, they are reported rather than absorbed. Marking this phase PASS
would require either asserting behaviour that does not exist or making an unapproved production
change.

### Required approvals to unblock

1. **B1** — authorise converting `IntegrityVerifier::verify_run` and its comparators to
   `anyhow::Result`, propagating through the 34 call sites. This is the same treatment `qa_memory.rs`
   received in Phase 6.1 R1b and should be scheduled as a remediation phase, not folded into
   validation.
2. **B2** — decide whether a `BLOCKED` step counts toward a case's execution history. If not,
   `test_stability` should exclude it from `executions` and return `failure_rate: None`. This is a
   product decision on what "execution" means.

### Recommended next action

**Do not start Phase 8.** Run a short **Phase 7.1 remediation** for B1 and B2 first: both are
narrow, both are already diagnosed with reproductions, and both touch surfaces Phase 8 (test-case
quality intelligence) would otherwise read as clean. Phase 8's own roadmap scope is
quality-intelligence scoring, which depends on executions being counted correctly — so B2 in
particular should be settled before anything scores test-case quality.

Phases 8, 9 and 10 were **not** started. Nothing was committed or pushed; all 22 pre-existing
modified files and all 7 untracked files are intact.