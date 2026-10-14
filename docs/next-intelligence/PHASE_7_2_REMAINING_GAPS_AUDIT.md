# PHASE 7.2 — Remaining Metric Contracts and Module-Change Gap Audit

**AUDIT ONLY. No production code, test behaviour, formula, weight, threshold, migration or route was
changed.**

**Finding headline:** the previous report said *"six additional sites"* and then listed **seven**.
Both numbers were wrong. The verified inventory is **15 sites across four distinct contracts** — and
one of them (`intelligence.rs::historical_failure_rate`) is **already correct**, which the previous
report did not identify. Two more are not failure rates at all.

**No user approval is assumed anywhere in this document.** Every recommendation is a proposal.

---

## 1. Current repository state

Verified at the start of this audit; reported values were treated as historical.

| | |
|---|---|
| Branch | `main` |
| HEAD | `7930507` — *fix: correct risk signal measurement handling* |
| Upstream | `origin/main`, **0 ahead / 0 behind** |
| Modified tracked files | **22** — identical to the start of this audit |
| Untracked files | **10** — identical to the start of this audit |

### Baseline re-measured in this run

| Check | Result |
|---|---|
| **Full backend suite** | **2032 passed / 0 failed / 5 ignored** |
| `cargo fmt --check` | 1 diff — `auth.rs:317`, pre-existing |
| `cargo clippy --all-targets` | exit 0 |

**Audit integrity:** `git status --porcelain` reports the same 22 modified and 10 untracked files at
the end as at the start, and no file in `services/orchestrator/src/` has a modification time inside
the audit window. Nothing was committed or pushed. All pre-existing work is intact.

Untracked files, all preserved:

```
AIQA_NEXT_ROADMAP_PHASED_PLAN.md
docs/next-intelligence/PHASE_6_1_OPEN_FINDINGS_AUDIT.md
docs/next-intelligence/PHASE_6_1_REMEDIATION_REPORT.md
docs/next-intelligence/PHASE_6_2_INTEGRATION_VALIDATION_REPORT.md
docs/next-intelligence/PHASE_7_USER_JOURNEYS_REPORT.md
docs/next-intelligence/PHASE_7_1_REMEDIATION_REPORT.md
services/orchestrator/src/integration_e2e_tests.rs
services/orchestrator/src/journey_tests.rs
services/orchestrator/src/phase_7_1_regression_tests.rs
services/orchestrator/src/risk_semantics_tests.rs
```

---

## 2. Corrected inventory of all failure-rate sites

I searched the whole of `services/orchestrator/src` for `failure_rate` (**158 occurrences: 85 in
non-test files, 73 in test files**) plus `rate(`, `/ rows.len()`, and `/ mine.len()`. Every
non-comment occurrence in non-test code was classified. Declarations, re-exports, serialisation
keys and `Factor::measured("failure_rate", …)` names were excluded; what remains is **14 genuine
calculations**.

The table answers all ten questions Part A asks. "Blocked contributes?" means: can a row whose
status is `BLOCKED`, `SKIPPED`, `CANCELLED` or any non-`PASSED`/`FAILED`/`TIMEOUT` value reach the
denominator?

| # | File:line | Function → consumer | Numerator | Denominator | `executions` means | Blocked contributes? | Can risk appear artificially low? | Contract type |
|---|---|---|---|---|---|---|---|---|
| **S1** | `qa_memory.rs` L434 / L485 / L547 | `test_stability` → `TestStabilityView.failure_rate` + `failure_rate` signal | `is_failure` = `FAILED\|TIMEOUT` | **`passed + failed`** (fixed in 7.1) | recorded outcomes (field); eligible (rate) | **No** | No — already fixed | **A. completed pass/fail rate** |
| **S2** | `qa_memory.rs` L718 / L772 | `module_risk_history` → field + `module_failure_rate` signal | `is_failure` | `mine.len()` | recorded outcomes | **Yes** | **Yes** — dilutes toward 0 | **B. all-outcomes statistics** |
| **S3** | `qa_memory.rs` L848 / L855 | `release_history` → `ReleaseComparison.failure_rate` | `is_failure` | `rows.len()` | recorded outcomes | **Yes** | **Yes** | **B** |
| **S4** | `qa_memory.rs` L864 / L899 | `release_history` → `ReleaseHistory.failure_rate` + signal | `is_failure` | `mine.len()` | recorded outcomes | **Yes** | **Yes** | **B** |
| **S5** | `qa_memory.rs` L978 / L1040 | `requirement_history` → field + `requirement_failure_rate` signal | `is_failure` | `mine.len()` | recorded outcomes | **Yes** | **Yes** | **B** |
| **S6** | `qa_memory.rs` L1388 / L1389 | `planning_signals` → `module_risk` signal | `is_failure` | `rows.len()` | recorded outcomes | **Yes** | **Yes** | **B** |
| **S15** | `qa_memory.rs` L1456 / L1457 | `planning_signals` → **`release_regression`** signal | `is_failure` | `rows.len()` (guarded `>= 2`) | recorded executions | **Yes** | **Yes — a regression signal read as cleaner than it is** | **B** |
| **S7** | `qa_memory_risk.rs` L288 / L289 | `module_risk` → `Factor::measured("failure_rate", …)` | `is_failure` | `rows.len()` | recorded outcomes | **Yes** | **Yes** | **B**, feeds **D3** |
| **S8** | `qa_memory_risk.rs` L469 / L470 | `release_risk` → `Factor::measured("failure_rate", …)` | `is_failure` | `rows.len()` | recorded outcomes | **Yes** | **Yes** | **B**, feeds **D3** |
| **S9** | `qa_memory_risk.rs` L1356 / L1360 | `test_risk_from` → `TestRisk.failure_rate` | `is_failure` | `rows.len()` | recorded outcomes | **Yes** | **Yes** | **B**, feeds **D3** |
| **S10** | `qa_memory_risk.rs` L346–351 | `module_risk` → `AffectedTestRisk.failure_rate` | `per_case_counts` failures | `per_case_counts` total | recorded outcomes | **Yes** | **Yes** | **B**, feeds **D3** |
| **S11** | `api.rs` L9190 / L9195 | project risk endpoint → `Factor::measured("failure_rate", …)` | `FAILED\|TIMEOUT\|**INFRA_ERROR**` | `total_runs` | recorded executions | **Yes** | **Yes** | **B**, feeds **D3**; different numerator |
| **S12** | `intelligence.rs` L517 / L530 | `update_test_intelligence` → `risk_assessments.historical_failure_rate` | `!Passed` over rows **already filtered** to `Passed\|Failed\|Timeout` | `window_runs` | **eligible executions only** | **No — filtered out** | **No** | **A — already correct** |
| **S13** | `operations.rs` L593 | `build_environment_matrix` → `EnvironmentOccurrence.failure_rate` | `recs.len()` (occurrences of a cluster) | `total_env_runs` | **occurrences of one failure signature** | Yes | Not a test failure rate | **C. different quantity** |
| **S14** | `api.rs` L9211 | retryable-rate metric | `INFRA_ERROR\|TIMEOUT` | n/a | n/a | n/a | n/a | **C. different quantity** |

### The four contract types

| Type | Definition | Sites |
|---|---|---|
| **A** | **Completed pass/fail failure rate** — "of the executions that actually ran and finished, how many failed" | S1 (fixed), **S12 (already correct)** |
| **B** | **All-recorded-outcomes statistics** — computed over every stored telemetry row regardless of status | S2–S11, S15 |
| **C** | **Not a failure rate** — occurrences of a failure signature (S13); retryable-error share (S14) | S13, S14 |

---

## 3. The six-versus-seven discrepancy, explained precisely

The Phase 7.1 report (§6, "Related finding") said:

> *"the same denominator exists in four other views — NOT changed"* → a 4-row table
> … *"Plus three sites in `qa_memory_risk.rs`"*
> … and elsewhere, *"the same rate denominator exists in 7 other places"*

Three separate errors:

1. **The arithmetic was wrong.** Four plus three is **seven**, not six. The count was never derived;
   the list was assembled by hand while writing prose, and the headline number was written first.
2. **The 4-row table was itself incomplete.** It listed `module_risk_history` (L751), `release_history`
   (L834), `release_history` (L878), `requirement_history` (L1019). Those are the *field*
   assignments only. The **signal** that each of those views also publishes recomputes the same rate
   independently — `qa_memory.rs` L718, L843, L978 — and `planning_signals` (L1389) is a fourth view
   the table omitted entirely. Counting signals and fields separately gives **7 sites in
   `qa_memory.rs`, not 4.**
3. **A whole view was missed twice.** `planning_signals` computes **two** rates over all rows: the
   `module_risk` signal (L1389) **and** the `release_regression` signal (L1457). The latter is the
   more consequential of the two — it is the signal that says *"this release regressed"*, and a
   blocked run pulls it toward zero.
4. **`api.rs` was never counted.** `api.rs` L9195 computes a project-level `failure_rate` from
   telemetry and feeds it into `calibrate_partial` as a measured factor. It is the *only* site whose
   numerator includes `INFRA_ERROR`, which makes it the most interesting of the lot and the one most
   worth discussing.

**Corrected count: 15 sites. 1 is fixed (S1), 1 is already correct (S12), 2 are not failure rates
(S13, S14), and 11 need a decision (S2–S11 plus S15).**

**My own first pass made the same class of error.** The table above initially listed 14 sites and
missed S15 (`release_regression`); a scripted re-scan of every `rate(` call site caught it. That is
the honest record of how this discrepancy happened, and the reason for the discipline in the
recommendation below.

The lesson: a count in a report should be produced by the same script that produced the table, and
never written before the list was.

---

## 4. Metric-by-metric contracts and recommendations

### Type A — completed pass/fail failure rate

**Rule:**
```
completed = passed + failed          // rows where is_pass(row) || is_failure(row)
failure_rate = failed / completed × 100   // None when completed == 0
```

**Why it is the intended measurement.** A failure rate answers *"how often does this thing fail
when it runs?"* A step blocked by a failed prerequisite never ran, so it is not evidence of
success or of failure. Including it in the denominator asserts a pass that did not happen. This is
the rule P7-D2 established and the one `intelligence.rs` already implements at S12.

**Blocked outcomes.** Excluded from both numerator and denominator. Still visible via
`other_outcomes`, and `executions` still counts them as recorded outcomes.

**Zero eligible observations.** `None`, never `0.0`. Existing `Option` convention. S1 and S12 both
already do this.

**Incomplete evidence.** A project with three factors unsourced cannot yield a canonical score —
that is `unmeasured_weight` and `calibration: None`, a separate mechanism from this rate.

**Supporting tests.** S1: `p71_d2_a_blocked_step_is_reported_as_blocked_and_its_rate_is_unmeasured`,
`p71_d2_non_execution_outcomes_do_not_distort_the_failure_rate_denominator`,
`p71_d2_insufficient_eligible_evidence_stays_unmeasured`,
`p71_d2_the_failure_rate_field_and_its_signal_always_agree`, plus
`qa_memory_retrieval_tests.rs:311` ("a 0% failure rate over one execution is arithmetically true").
S12: `intelligence.rs`'s own path, exercised by `intelligence_tests`.

**Consumers that would be affected if extended.** None — both Type A sites already comply.

### Type B — all-recorded-outcomes statistics (S2–S11, S15)

**This is where a real decision is needed, and where a blanket change would be wrong.**

**Option B1 — reclassify them as Type A.** Make all ten use `passed + failed`.

- *Why it is right:* the field is named `failure_rate` and reads as "this fails N% of the time".
- *Blocked outcomes:* excluded; visible via counts.
- *Zero eligible:* `None` — but **S7/S8/S11 currently return a bare `f64` clamped 0..1**, not
  `Option`. Making them `Option` is a **public API change** to `Factor::measured`, `TestRisk`,
  `ReleaseRiskView`, `ModuleRiskView` and the risk endpoint's JSON.
- *Incomplete evidence:* unchanged.
- *Tests that would break:* `qa_memory_retrieval_tests` (6 assertions), `qa_memory_risk_tests` (7),
  `risk_prioritisation_tests` (11), `api_tests` (5), `journey_tests` + `phase_7_1` (33 nearby).
- *Downstream:* the JSON shape of at least three endpoints changes. `ai_generation.rs` consumes
  `TestIntelRow`; `change_impact.rs::risk_scores` reads it for the high-risk selection tier.
- **Cost: high. Risk: low. Reversibility: low once shipped.**

**Option B2 — keep the denominators, and rename/relabel to say what they measure.**

- *Why it is defensible:* over a long history, "of all recorded attempts, what fraction failed" is a
  legitimate **reliability-per-attempt** figure, and it is what these sites compute.
- *But it is not defensible to leave the name alone:* a reader of
  `TestRisk.failure_rate` cannot tell it from S1's, and the two answer different questions.
- *Blocked outcomes:* counted — which must then be **stated in the field's documentation**, because
  today no field documents it.
- *Zero eligible:* cannot occur (denominator ≥ 1 by construction).
- *Cost: low. Risk: none. Reversibility: full.*

**Option B3 — split the difference: fix the *named* metric, leave the aggregate alone.**
Change `failure_rate` (the field consumers read) to Type A, and add a **new, separately named**
field for the all-observed-outcomes figure (e.g. `attempt_failure_rate`) carrying today's value.

- *Why it is best:* it makes both quantities available, names them honestly, and keeps the reported
  "failure rate" meaning what a reader expects.
- *Cost:* high — a new field on several public structs, plus tests.
- *Downstream:* additive, so old consumers keep compiling; but it doubles the surface a reader must
  understand, which is a real cost in a system whose stated problem is untrustworthy metrics.

**Recommendation: B3 if budget allows, B2 otherwise. Not B1** — B1 changes the meaning of a
risk-weighted field without adding the information that would make the change safe to interpret,
and it forces `Option` through three public risk types while **D3 is open**.

### Type C — not failure rates (S13, S14)

**S13, `operations.rs:593`.** Numerator is `occurrences` = *the number of telemetry rows carrying one
failure signature in one environment*; denominator is `total_env_runs` for that environment's
affected tests. This is **"what share of this environment's runs hit this specific defect"**, not a
test pass rate. Its `else { 1.0 }` branch (no runs ⇒ 100% failure) is a **worst-case sentinel**,
deliberate in an investigation view. **Recommendation: leave it. Document the sentinel.**

**S14, `api.rs:9211`.** Share of `INFRA_ERROR | TIMEOUT` — a **retryability** signal feeding the
executor/retry path. Not a failure rate. **Recommendation: leave it.**

**S11's numerator deserves its own note.** It counts `INFRA_ERROR` as a failure; S1–S10 do not.
`INFRA_ERROR` means *the executor itself broke* (no browser, per `autopilot_execute.rs`) — an
infrastructure fault, not a product or test failure. Including it inflates the project's failure
rate for conditions the application under test never caused. This is the **opposite** direction of
the dilution problem, and it is a scope question, not a denominator question.

---

## 5. Downstream impact of each possible denominator change

| Change | Public API shape | Endpoints affected | Other consumers | Migration | Existing tests at risk |
|---|---|---|---|---|---|
| S2–S6, S10 to Type A | `Option<f64>` for S2–S5; S10 already `Option` | change-impact, risk views, planning signals | `change_impact.rs::risk_scores` reads `TestIntelRow` | **none** | ~20 exact-value assertions |
| S7–S9, S11 to Type A | `Factor.measured` → `Factor::unmeasured`; `TestRisk.failure_rate` → `Option`; `ReleaseRiskView.failure_rate` → `Option` | project risk, module risk, release risk | `calibrate_partial` normalisation, `ai_generation.rs` | **none** | ~23 exact-value assertions |
| B2 (rename/relabel only) | docs and field docs; optional alias | none | none | **none** | **0** |
| B3 (add a field) | one additive `Option<f64>`/named field per view | response bodies gain a key | none | **none** | 0 existing; new tests needed |

**No option requires a migration.** Every figure is computed at read time from
`execution_telemetry`; nothing is persisted except `risk_assessments.historical_failure_rate`, which
comes from S12 and is already correct.

---

## 6. Module-change resolution trace

### The pipeline, step by step, with the stop point marked

| Step | Operation | Location | Result for a `module` target |
|---|---|---|---|
| 1 | Declare change | `POST /api/v1/projects/:id/change-impact` → `ChangeImpactReader::analyse` | **accepted**, HTTP 201 |
| 2 | Persist impact evidence | `analyse` → `insert_change_impact` | **nothing written** — the `let Resolution::Resolved … else { continue }` at L692 skips unresolved targets, so a module change leaves **no row at all** |
| 3 | Target resolution | `resolve` L279 → `resolution_note("module")` L93 | **STOPS HERE.** Returns `Resolution::Insufficient` with *"a module vertex exists, but no `covers` edge is ever written, so a module change has no provable impact on any test case"* |
| 4 | Graph traversal | `impacted_from_node` L368 | never reached |
| 5 | Candidate discovery | same | never reached |
| 6 | Regression selection | `analyse` L627–L669: safety floor + high-risk tier still applied | runs, but **only** the change-independent tiers — no change-derived selection exists |
| 7 | Report | `insufficient_reason` set when `!any_resolved` | *"no declared target resolved to a stored graph vertex in this project, so no impact could be derived. This is reported rather than guessed."* |

**The exact stop point is step 3.** Note it is *earlier* than previously reported: the journey does
not reach graph traversal. And a module change writes **no** `change_impacts` row, so a later query
for that change cannot even tell it was attempted — only the synchronous response carries
`insufficient_reason`.

### Verifying the reported claims

| Claim | Verdict |
|---|---|
| `resolution_note("module")` refuses | **CONFIRMED** — verbatim, `change_impact.rs` L93 |
| No production `covers`-edge writer | **CONFIRMED, and more strongly than stated.** `Store::link_test_case_to_module` (L1049) and `Store::link_case_to_module` (L1133) both exist and both write `covers`, but **no production code calls either** — only `trace.rs` mentions them, in a module-level comment. `trace.rs`'s own header states: *"`covers` had no production caller — and still has none, **deliberately**"* |
| No authoritative module identity source | **CONFIRMED by schema inspection.** I grepped all 32 migrations for `module`/`component`/`package`/`file_path`: the only hit is `0029_qa_releases.sql`, and it is unrelated. `trace.rs` records the same finding from the Phase 1 work: *"`TestCaseRow` carries `source_document` (a document, not a module), `TaskDef` has no module field, and `RunPlan` has no file or path"* |
| Related unsupported targets | **CONFIRMED and wider than reported.** `resolution_note` refuses **seven** kinds, not three: `module`, `api_contract`, `page`, `code_file`, `configuration`, `dependency`, `requirement_change`. Only `requirement`, `test_case` and `failure_cluster` resolve. |

### Does coverage evidence exist anywhere? (checked, not assumed)

A table being *able* to store a relation is not evidence that anything stores one. I checked:

- `store::link_case_to_module` — **exists, id-keyed, tested**, called by production code: **none**.
- `list_quality_relations_for_project` — exists; `trace.rs` reports `quality_relations` held **0
  rows** in the live database at Phase 1, and states the writers are the reason.
- `quality_nodes` with `node_type = "module"` — writable via `upsert_module_node`; exercised by
  `qa_memory_tests.rs` L713/L745 and `trace_tests.rs` L347/L729 **as fixtures only**.
- Test case schema: `source_document` exists (a document name). **No module, component, package or
  file-path column anywhere.**

**Conclusion: a `module` vertex can be created by a test and cannot be created by any supported user
action. Wiring a `covers` edge today would mean inferring ownership from a document name or a task
name — a heuristic presented as fact, which `trace.rs` explicitly forbids and which this audit's
rules also forbid.**

---

## 7. Options for module-change support

### OPTION A — Implement support

Required, and none of it is a small change:

1. **Authoritative module identity.** A new column or table. The decision is *where module identity
   comes from*: `test_cases.module_ref` (explicit, user-supplied), a code-scan manifest, or a
   document→module mapping. Only the first is not a heuristic. **This is the blocking design
   decision — it does not exist today in any form.**
2. **Production writer.** A call site that writes `covers` from (case, module) whenever a case is
   created or updated. Today both writers are uncalled.
3. **Schema.** `ALTER TABLE test_cases ADD module_ref` — a migration, which this phase may not add.
4. **Reader + selection.** `resolve("module")` already looks up `node_type = "module"` by
   `external_ref`, and `impacted_from_node` already traverses `covers` reverse edges. **Both are
   already implemented and would work the moment edges existed.** That is the one genuinely cheap part.
5. **Tests.** A case+module fixture proving the edge is written by a supported path; a module-target
   change asserting selection; a negative test that an unlinked module selects nothing; tenant
   isolation across two projects; the existing `journey_c` refusal assertion must be **rewritten**.

**Honest assessment: A is one migration + one writer call site + tests, *conditional on* decision 1.
Without a decision on where module identity comes from, A cannot be started, and any implementation
that guesses would be exactly the heuristic link `trace.rs` refuses to create.**

### OPTION B — Formally defer

Make the refusal an accepted product limitation:

1. **API surface.** Keep `module` accepted by the endpoint (a 400 would break clients) but always
   return `insufficient_reason` naming the missing capability. **This already happens.**
2. **The real gap to close: step 2 writes no `change_impacts` row.** A refused change is
   invisible to any later query. Deferral is only honest if the attempt is *recorded* as refused. That
   needs a store call — a code change.
3. **Documentation.** `resolution_note`'s table already names the missing edge per kind. Extend it to
   state the *schema* decision that would close each gap, so the next reader does not re-derive it.
4. **Acceptance criteria.** "A change kind this build cannot resolve produces an explicit
   `Insufficient` reason naming what is missing, selects nothing, and claims nothing." All four parts
   are testable and three already hold.
5. **Tests.** A test asserting a module change writes **zero** `change_impacts` rows *and* returns a
   reason — i.e. asserting the refusal is honest, which is currently untested.
6. **Roadmap consequence.** See below.

### OPTION C — Clarify the requirement

The ambiguity is real and I can point at it precisely.

Phase 7's required test list says **"journey selection after a supported change"**, and capability 8
says **"selecting journeys affected by a *supported* change-impact record"**. The word **"supported"**
is doing all the work, and it is **defined nowhere in the roadmap**. Two readings:

- **Reading 1 (strict):** every change kind in `resolution_note`'s accept-list must resolve.
  `module` is in `resolve`'s accept-list (L303) yet refused by `resolution_note` — so under this
  reading Phase 7 cannot pass, which is the verdict Phase 7 reached.
- **Reading 2 (permissive):** "supported" means "a change kind this build documents as supported",
  i.e. `requirement`, `test_case`, `failure_cluster`. Phase 7's required test is then satisfied — and
  it was: `journey_c_change_to_regression_selection` exercises a `requirement` target end to end and
  passes 10/10.

**Reading 2 is what the code implements and what the Phase 7 report's own test matrix recorded.** But
the roadmap never says so, and `resolve`'s own accept-list contradicts it by including `module`. That
contradiction is the defect worth fixing in the requirement: **the accept-list and the refusal table
disagree about whether `module` is supported.**

**The decision required:** declare the canonical supported-kind set, and make `resolve`'s
`matches!(kind, …)` and `resolution_note` derive from it rather than being two hand-written lists.

---

## 8. D3, D5 and D6 status

### D3 — canonical risk formula: **OPEN**, and this audit sharpens it

**Established.** Two distinct scorers exist and neither is canonical:

| | `qa_memory_risk::calibrate_partial` | `adaptive_intelligence::calibrate_risk` |
|---|---|---|
| Location | L865 | L808 |
| Inputs | `Factor` list with per-factor `Option` values | `RiskFactors` struct of bare `f64` |
| Weights | `FACTOR_WEIGHTS` (L59): failure_rate 0.35, flakiness 0.20, severity 0.15, dependency 0.10, healing 0.10, security 0.10 | inline, same six at the same values |
| Missing data | normalised over measured weight; explains the assumption | **substitutes 0.0 silently** |
| Nothing measured | `INSUFFICIENT_DATA`, score 0.0, explicitly *"this is not a score of zero"* | returns a real 0.0 score |

**`failure_rate` is the single heaviest factor at 0.35.** S7, S8, S9, S10 and S11 all feed it.
**That is why the S2–S11 denominator question cannot be settled inside D3's shadow:** changing a
denominator changes the heaviest input to an open model.

Also newly established: **three of six factors (0.30 of the model) have no column anywhere**, so
`unmeasured_weight ≈ 0.30–0.45` is **structural, not data-dependent**. No amount of execution
evidence produces a canonical score. D3 is not blocked on data.

**Still unknown.** Which scorer is authoritative; whether `calibrate_risk`'s silent `0.0`
substitution is intended; whether `INFRA_ERROR` belongs in the numerator (S11).

**Can Phase 8 proceed?** The audit says **yes, conditionally** — Phase 8's own scope is
test-case-quality scoring, which is *itself* a risk-adjacent scoring problem. Starting it while the
heaviest factor's denominator is undefined would encode the ambiguity into a new scorer. **Settle
S2–S11's contract first.**

### D5 — 10 ms duration floor: **OPEN**, unchanged, verified

**Established.** `DURATION_ABSOLUTE_FLOOR_MS = 10.0`, `DURATION_RELATIVE_TOLERANCE = 0.10`,
combined by `duration_tolerance_ms` as `.max()`. The module's own documentation argues the case: both
sides are integer milliseconds, so under a relative tolerance alone a 0 ms span is infinitely
sensitive and the verdict becomes a function of write timing.

**Still unknown, and the point stands:** the source says *"It is a stated policy constant, not a
value fitted to any observed dataset."* That is a **rationale, not an endorsement**. There is no
recorded measurement of the real inter-write gap distribution in this deployment, so "10 ms is ten
times the 1 ms quantum" argues internal consistency, not empirical adequacy. Tests pass; passing
tests confirm the implementation matches its own stated intent.

**Can Phase 8 proceed?** **Yes.** D5 affects only cross-layer duration comparison, which is not a
Phase 8 input.

**Decision required if anyone wants it closed:** measure the actual backend-vs-API duration
distribution in a representative run, and set the floor from that — or record an explicit product
decision that 10 ms is the accepted policy regardless.

### D6 — `set_task_status` API: **OPEN**, and this audit quantified it

**Established.** The signature is `(run_id, task_id, status, reason, set_started: bool,
set_finished: bool)`. The body is an `if / else if / else` over the two flags that selects **three
different SQL statements**, and the first two branches **do not write `reason`**. Call sites by
flag combination:

| `(set_started, set_finished)` | total | production | tests | meaning |
|---|---|---|---|---|
| `(true, false)` | 22 | 2 (`engine.rs` L838, `store.rs` L5903) | 20 | start only |
| `(false, true)` | 19 | 2 (`engine.rs` L508, L654; `store.rs` L5907) | 17 | finish only |
| `(false, false)` | 1 | **0** | 1 | status + reason, **no timestamp** |
| `(true, true)` | 2 | **0** | 2 | unreachable: `if/else if` |
| variable | 6 | 1 | 5 | — |

**Two findings beyond the enum question.** First, `(true, true)` is **unreachable** — the `else if`
means only one statement ever runs, so no call can set both timestamps in one statement.
Second, `(false, false)` — the branch that sets a status with **no timestamp** — has **no production
caller**; it exists only in a test. So production uses exactly two shapes, which is the strongest
argument yet for a small enum.

**Can Phase 8 proceed?** **Yes**, with a caveat: if Phase 8 records execution timestamps, it will
call this method, and it should not be written before the API is settled. Note that P7-D1's
`recorded_at IS NOT NULL` question (Phase 6.1 F2) touches the same rows.

**Decision required:** replace the two booleans with an enum (`Start` / `Finish` / `StatusOnly`), or
keep them and document the unreachable branch. Either is small; doing it now avoids Phase 8 writing
against a signature known to be awkward.

---

## 9. Tests required for each proposed remediation

**None of these were written — this phase is audit-only.**

### For the metric contracts (S2–S11)

| # | Test | Purpose |
|---|---|---|
| T1 | blocked outcome excluded from `module_risk_history.failure_rate` | S2 |
| T2 | blocked outcome excluded from `release_history` field **and** per-release comparison | S3, S4 |
| T3 | blocked outcome excluded from `requirement_history` field **and** signal | S5 |
| T4 | blocked outcome excluded from `planning_signals` `module_risk` **and** `release_regression` metrics | S6, **S15** |
| T5 | blocked outcome excluded from `module_risk` / `release_risk` **factors**, and the factor becomes `unmeasured` at zero eligible rows | S7, S8 — needs an `Option` decision first |
| T6 | blocked outcome excluded from `test_risk_from`, and `TestRisk.failure_rate` is `None` at zero eligible | S9 — same |
| T7 | `AffectedTestRisk.failure_rate` excludes blocked rows | S10 |
| T8 | project risk endpoint excludes blocked rows; **`INFRA_ERROR` treatment explicitly asserted** | S11 |
| T9 | **for every site: field and its signal/trend point agree**, and `executions` still counts blocked rows | guards the S1 regression class |
| T10 | genuine pass-only ⇒ 0%; genuine fail-only ⇒ 100%; 3:1 ⇒ 25% at each new site | proves no regression of valid rates |
| T11 | zero eligible rows ⇒ `None`/unmeasured, never `0.0` | |
| T12 | project isolation: two projects with identical outcomes keep separate rates | |
| T13 | **S12 unchanged** — `historical_failure_rate` still filters to eligible-only | proves the fix did not regress the already-correct site |
| T14 | S13's `else { 1.0 }` sentinel still returns 1.0 with no runs | proves Type C was left alone |

**Mutation requirement:** each of T1–T8 must be shown to fail when its denominator is reverted —
the discipline Phase 7.1 used, including recording mutations that are *not* caught.

### For the module-change path

| # | Test | Purpose |
|---|---|---|
| M1 | a `module` change returns `Insufficient` naming the missing `covers` edge | exists in `journey_c` |
| M2 | a `module` change selects **zero** tests and claims nothing | exists in `journey_c` |
| M3 | **a refused change writes zero `change_impacts` rows and is still reported** | **missing** — closes the Option B honesty gap |
| M4 | the refusal reason is identical for all seven refused kinds and names a *schema* decision | missing |
| M5 | `resolve`'s accept-list and `resolution_note` cannot disagree | missing — guards Option C's fix |
| M6 | if Option A is ever approved: a case created with `module_ref` writes a `covers` edge **through a supported path**, and a module change then selects that case | future |
| M7 | if Option A is approved: a module with no linked case selects nothing and says so | future |

### Suites to run after any approved implementation

`phase_7_1_regression_tests` · `journey_tests` · `integration_e2e_tests` · `integrity_tests` ·
`qa_memory_retrieval_tests` · `qa_memory_risk_tests` · `qa_memory_planning_tests` ·
`qa_memory_validation_tests` · `qa_memory_tests` · `risk_prioritisation` · `risk_semantics_tests` ·
`risk_signal_tests` · `change_impact_tests` · `adaptive_intelligence_tests` · `intelligence_tests` ·
`operations_tests` · `store_tests` · `engine_tests` · `api_tests` · `trace_tests` · `exploration_tests`
· then the full backend suite, `cargo fmt --check`, `cargo clippy --all-targets`.

---

## 10. Recommendation and the decisions requiring your approval

### Recommended path

1. **Decide the metric contract for S2–S11 and S15 as one question, not eleven.** Recommend **B3** (additive
   named field) if the budget allows, **B2** (keep and document) if not. **Not B1.**
2. **Decide the module-change requirement (Option C), then B or A.** Recommend **C now**, because
   the accept-list/refusal-table contradiction is a defect regardless of which path follows. Then
   **B** — formal deferral with M3–M5 added — unless you want module identity as a real product
   feature, which is **A** and needs its own design decision first.
3. **Then Phase 8.** Not before: Phase 8 scores test-case quality, and it would inherit the exact
   ambiguity that makes `failure_rate` (weight 0.35) currently mean two different things.

### Decisions requiring your approval — none assumed

| # | Decision | Why it needs you |
|---|---|---|
| **Q1** | Metric contract for S2–S11 and S15: **B3** (add a named field) / **B2** (document only) / **B1** (change the denominator) | changes public API shapes and JSON; B1 is entangled with D3 |
| **Q2** | Should `INFRA_ERROR` count as a failure in S11? | semantics: an executor fault is not a test failure. Affects only S11 |
| **Q3** | Is module-change support mandatory for Phase 7, or is the supported set `{requirement, test_case, failure_cluster}`? | the roadmap says "supported change" and never defines it |
| **Q4** | If module support is wanted: where does module identity come from — explicit `test_cases.module_ref`, code scan, or document mapping? | only the first is not a heuristic; determines whether Option A is viable |
| **Q5** | Fix the `resolve` accept-list / `resolution_note` contradiction, and in which direction? | either drop `module` from the accept-list, or make the tables share one source |
| **Q6** | D6: replace the two booleans with an enum now, or document the unreachable branch? | small, but it is an API change; better done before Phase 8 writes new callers |
| **Q7** | Should a refused change persist a record? (Option B's M3) | honesty gap; needs a store change |

**D3, D5, D6 all remain OPEN.** No weights, thresholds or formulas were changed by this audit, and
none is proposed for change here.

**Phases 8, 9 and 10 were not started. Nothing was committed or pushed.**