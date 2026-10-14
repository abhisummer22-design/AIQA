# PHASE 6.2 — Real-Data End-to-End Integration Validation

**Phase:** PHASE 6.2 — REAL-DATA END-TO-END INTEGRATION VALIDATION
**Verdict:** **PASS** — with three honest limitations recorded in §12 and two open decisions (§13).
**Production code changed: none.** One new test module plus one `mod` line.

---

## 1. Initial repository state

Verified at the start of this phase, not carried over from the prior report.

| | |
|---|---|
| Branch | `main` |
| HEAD | `7930507` — *fix: correct risk signal measurement handling* |
| Upstream | `origin/main`, **0 ahead / 0 behind** |
| Modified tracked files | 22 |
| Untracked files | 4 (`AIQA_NEXT_ROADMAP_PHASED_PLAN.md`, `PHASE_6_1_OPEN_FINDINGS_AUDIT.md`, `PHASE_6_1_REMEDIATION_REPORT.md`, `risk_semantics_tests.rs`) |

All 22 modified files and all 4 untracked files are the Phase 6.1 remediation plus the F2
test-stability follow-up, both already reported. They were **preserved unchanged**. Nothing was
committed or pushed in this phase.

### Baseline re-measured

| Check | Result |
|---|---|
| Full backend suite (before any Phase 6.2 edit) | **1984 passed / 0 failed / 5 ignored** |
| Compile errors | 0 |
| Migrations on disk | 32 |

---

## 2. Scope taken from the roadmap

From `AIQA_NEXT_ROADMAP_PHASED_PLAN.md` §PHASE 6.2:

> **Target chain:** `Requirement → Test Case → Change → Impact → Prioritized Test → Execution →
> Evidence → Failure → Trace Graph → Risk / Integrity result`

12 items to validate, 10 required scenarios, and a PASS gate requiring *real persisted records
created through supported writers*. All 12 and all 10 are addressed below.

### Mandatory scenarios → where each is covered

| Roadmap scenario | Scenario | Status |
|---|---|---|
| one completely passing journey | S1 | PASS |
| one controlled failure with recorded evidence | S2 | PASS |
| one supported change that affects related tests | S3 | PASS |
| one unrelated change that must not invent impacts | S4 | PASS |
| one missing trace link | S4, S9c | PASS |
| one risk signal truly measured as zero | S6a | PASS |
| one risk signal unmeasured | S6b | PASS |
| duplicate failure pattern across two projects, no leakage | S5, S5b | PASS |
| cross-layer mismatch, same entity, aligned window | S7a | PASS |
| stale/ambiguous observation that must not become a confirmed defect | S7b | PASS |

### Validate items 1–12 → covered

| # | Item | Scenario |
|---|---|---|
| 1 | Requirement-to-test traceability | S1 |
| 2 | Stable test identity through execution | S1 |
| 3 | Execution telemetry and failure-fingerprint consistency | S2 |
| 4 | Failure-to-bug relation where a real bug record exists | S10b |
| 5 | Change declaration and change-impact records | S3 |
| 6 | Smart regression selection | S3 |
| 7 | Risk prioritisation and unavailable-signal handling | S6a–c |
| 8 | Exploratory execution budget and action-policy enforcement | S8 |
| 9 | Evidence persistence and duplicate-finding control | S8, S10a |
| 10 | Cross-layer comparisons only when identity and timestamps support them | S7a, S7b |
| 11 | Project isolation across two independent projects | S5, S5b |
| 12 | Empty-history and insufficient-data handling | S6c, S9e |

---

## 3. Architecture and data-flow map

Traced from source, not assumed. Entry point → writer → persistence → reader → consumer.

| Capability | Entry point | Production writer | Persisted as | Production reader | Downstream consumer |
|---|---|---|---|---|---|
| Requirement | `POST .../test-cases` with a reference; document ingest | `requirements::parse_document` → `upsert_requirement_document` | `requirement_documents`, `requirements` | `list_requirements` | `GET .../requirement-impact` |
| Test case | `POST .../test-cases` | `Store::insert_test_case` | `test_cases` | `list_test_cases_for_requirement` | traceability + change impact |
| Trace link | same request | `Store::link_case_to_requirement` | `quality_nodes` + `validates` edge | `ChangeImpactReader::resolve` | regression selection |
| Run + tasks | execution | `Store::insert_run_for_release` | `runs`, `tasks` | `list_tasks_for_run` | integrity verifier |
| Task status | execution | `Store::set_task_status` (two calls) | `tasks.status/started_at/finished_at` | `IntegrityVerifier` observations | cross-layer comparison |
| Telemetry | `Intelligence::ingest_run` | `ExecutionTelemetry::from_task_state` → `insert_execution_telemetry` | `execution_telemetry` | `QaMemoryReader::telemetry_for` | QA Memory, risk, change impact |
| Failure + fingerprint | `ingest_run` | `canonical_failure_fingerprint`, `record_failure` | `failure_clusters` | `get_failure_cluster_by_fingerprint` | bug history, safety floor |
| Bug record | explicit review call | `Store::insert_bug_draft` | `bug_drafts` | `list_bug_drafts` | safety floor, `bug_history` |
| Evidence | execution | `LocalEvidenceStore::put` → `insert_evidence` | `evidence` + file on disk | `get_evidence_by_id`, `get_evidence_object_key` | `GET .../evidence/:id` |
| Change declaration | `POST .../change-impact` | `ChangeImpactReader::analyse` → `insert_change_impact` | `change_impacts` | `prioritise_tests` | `Changed` priority band |
| Regression selection | `GET .../test-risk`, `intelligence/tests` | `QaMemoryReader::prioritise_tests` | derived from `change_impacts` + telemetry | `TestPrioritisation` | planning |
| Risk | `GET .../test-risk` | `QaMemoryReader::test_risk` | derived | `TestRisk` | bands, planning feedback |
| Exploration | `GET .../exploration` | `insert_exploration_session`, `record_exploration_finding` | `exploration_sessions`, `exploration_findings` | `list_exploration_findings` | exploration endpoint |
| Integrity | `GET .../integrity/:run_id` | `IntegrityVerifier::verify_run` | derived | `IntegrityReport` | defect count |

### Test database and environment

- **Isolated per scenario.** Every scenario calls `Store::connect`, which runs
  `sqlx::migrate!("./migrations")` — all **32 migrations on a fresh SQLite file** per scenario. No
  schema is hand-built and no migration is bypassed.
- WAL journal, `busy_timeout` 5 s, foreign keys on — the production `connect` options.
- **No external service, credential, or dataset is required or used.** `SimulatedExecutor` and an
  inert AI router (a `FakeExecutor` returning a fixed body) are used so `AppState` can be
  constructed; no network call is ever made. No production data was touched.
- API-key material is generated in-process by `auth::generate_key`; only hashes are stored. No secret
  appears in this report.
- Evidence objects are written under the scenario's own `TempDir`.

### What was deliberately NOT done

No direct SQL was used to manufacture state. The one place raw SQL appears
(`insert_execution_telemetry` in S7b) is a **public production writer** whose struct field
`created_at` governs observation time; the row is built by the production constructor
`ExecutionTelemetry::from_task_state`, the same constructor `ingest_run` calls.

---

## 4. Integration scenarios — the new module

`services/orchestrator/src/integration_e2e_tests.rs`, 20 tests, registered in `lib.rs`.

| # | Scenario | What it proves end to end |
|---|---|---|
| S1 | Requirement → case → execution → QA Memory → integrity | the whole passing chain, over 6 real writers and 3 real readers |
| S2 | Controlled failure | fingerprint, cluster, evidence row + object, all agreeing on one case |
| S3 | Declared change | `change_impacts` rows persisted over HTTP; prioritisation honours them |
| S4 | Unrelated change / unsupported target | no invented impacts; `code_file` refused with a reason; viewer forbidden |
| S5 | Duplicate pattern, two projects | identical fingerprints, two clusters, zero cross-reads |
| S5b | HTTP project scoping | a project-scoped key cannot read another project |
| S6a | Measured zero | `failure_rate` is `Some(0.0)` citing 2 evidence rows |
| S6b | Unmeasured signal | `security_sensitivity` stays `None` with a basis and 0 evidence |
| S6c | Failed read vs empty | 8 consumers fail on a closed store; both honest empty states distinct |
| S7a | Genuine cross-layer mismatch | detected, named, defect established, identity from the stored key |
| S7b | Stale observation | 48 h skew → `InsufficientData`, **0 defects established** |
| S7c | The 10 ms floor | its actual behaviour at both boundaries and the crossover |
| S8 | Exploration | budget persisted, counters real, duplicate control, provenance to the API |
| S9a | Read failure | 8 consumers + planning feedback all fail loudly |
| S9b | Write failure | fails, and prior state is untouched |
| S9c | Missing link | unresolved target refused; unknown case is `UnknownSubject` |
| S9d | Repeated execution | 4 runs → 4 rows, 1 case, 1 fingerprint, rate across all four |
| S9e | Incomplete telemetry | non-terminal task is not an execution; absence named |
| S10a | Evidence read-back | served by id and content; project-scoped; 404 for unknown id |
| S10b | Failure-to-bug | real draft → safety floor reason and `bug_history` |

---

## 5. Scenario-by-scenario status

| Scenario | Status | Note |
|---|---|---|
| S1 | **PASS** | real parser → `FR-001/002/003`; link resolves to the stored case id; telemetry keyed on `case`; QA Memory `executions == 1`; integrity 0 inconsistent |
| S2 | **PASS** | fingerprint non-empty; cluster keyed on it; evidence row + object both readable; QA Memory reports the same fingerprint |
| S3 | **PASS** | 201 on first declaration, 200 + `already_recorded` on repeat; `Changed` band derived from the persisted row |
| S4 | **PASS** | empty impact set; `not_selected` carries a reason; `code_file` → `Insufficient` naming git/diff; viewer → 403 |
| S5 | **PASS** | fingerprints proven identical, clusters distinct, crossed read `UnknownSubject` |
| S5b | **PASS** | cross-project GET refused; owning key works |
| S6a | **PASS** | `failure_rate == Some(0.0)` with `evidence == 2` |
| S6b | **PASS** | `security_sensitivity` unmeasured, basis non-empty, `evidence == 0`, no calibration |
| S6c | **PASS** | `NoExecutions` vs `NoMatchingHistory` distinguished; 3 views error on close |
| S7a | **PASS** | `Inconsistent`, both values named, `defects_established == 1`, `run_task_key` identity |
| S7b | **PASS** | 48 h skew → `InsufficientData` with the skew reported; **0 defects established** |
| S7c | **PASS** | floor 10.0, inclusive at 10.0, exclusive at 10.001, crossover at 100 ms |
| S8 | **PASS** | budget + counters + named stop reason persisted; partial unique index suppression; provenance reaches the API |
| S9a | **PASS** | 8 consumers + planning feedback all error |
| S9b | **PASS** | write fails; prior row unchanged |
| S9c | **PASS** | missing target `Insufficient`; unknown case `UnknownSubject` |
| S9d | **PASS** | 4 executions, 1 fingerprint, rate 50 % across all four |
| S9e | **PASS** | non-terminal task produces no telemetry; `NoExecutions`, 0 defects, layers named |
| S10a | **PASS** | detail + content served; cross-project refused; unknown id → 404 |
| S10b | **PASS** | before: no bug history; after: exactly one recurring defect, pointed at the real cluster |

---

## 6. Failures reproduced, and one honest non-reproduction

**No production defect was found in Phase 6.2.** Every failure below was in my own new test code,
found and corrected before the suite was green. They are recorded because the corrections changed
what the scenarios actually assert.

| Symptom | Root cause | Correction |
|---|---|---|
| requirement parser found nothing | a bare `### FR-001 …` heading is a *section*, not a requirement | wrote the document the way the parser's own fixtures do: ids in list items |
| `cluster.project_id` absent | `FailureClusterRow` carries no project column — it is reached through a project-scoped query | asserted `occurrence_count` instead; project scope is proven in S5 |
| `change_impacts` recorded nothing | `analyse` needs a `test` graph vertex keyed on the case id; a case row alone is not enough | added `seed_case_vertex`; documented that the table stores **node** ids, so nodes are mapped back via `external_ref` |
| out-of-window skew not produced | `ingest_run` stamps both layers "now", so a second fresh row is matched first | write **only** the aged row, via the production writer |
| duplicate finding not suppressed | suppression is a partial unique index on `(project_id, failure_fingerprint) WHERE … IS NOT NULL`; a NULL fingerprint is deliberately exempt | asserted both directions: a fingerprinted finding suppresses, an `INSUFFICIENT_EVIDENCE` row never does |
| staleness outcome expected wrong | the verifier expresses "cannot answer" as `InsufficientData`; `NotComparable` means different subject matter | asserted `InsufficientData` and stated why in the comment |

**A non-reproduction worth recording.** My first attempt at a change-impact mutation — making the
`validates` edge's target case ignored — **did not fail the suite**. Rather than accept the green
result, I checked whether the mutation had actually applied; it had not (a script asserted before
writing and aborted). Re-applied via the `edit` tool and targeting the real invariant — seeding
`found` with *every* case — and it **did** fail S3 with
`a case linked to an unchanged requirement must not be`. The lesson is the same one from Phase 6.1:
a passing mutation test is evidence of nothing until you have seen it fail.

---

## 7. Discrimination — verified by mutation, not assumed

Each mutation reintroduces a real defect. All were reverted and the files verified byte-identical
(SHA-256 match).

| Mutation | Result |
|---|---|
| `test_stability` masks its telemetry read again (undoing R1b) | **caught** — `s9a` fails |
| `list_execution_telemetry` ignores the project filter | **caught** — 6 scenarios fail, incl. S5 isolation |
| change impact no longer requires a stored edge | **caught** — `s3` fails |

Verified after restore: `change_impact.rs`, `qa_memory.rs`, `store.rs` contain no mutation markers,
and `integration_e2e_tests` passes 20/20.

---

## 8. Test results — this run

| Check | Command | Result |
|---|---|---|
| New scenarios, each ×10 | `cargo test integration_e2e_tests::<name>` | **18/18 scenarios 10/10** |
| New module ×20 | `cargo test integration_e2e_tests` | **20 passed / 0 failed** |
| New module ×20 after probe removal | same | **20 passed / 0 failed** |
| Integrity suite ×20 | `cargo test integrity_tests` | **20 passed / 0 failed** |
| **Full backend suite** | `cargo test` | **2004 passed / 0 failed / 5 ignored** |
| `cargo fmt --check` | | 1 diff — `auth.rs:317`, pre-existing at HEAD |
| `cargo clippy --all-targets` | | **exit 0** |
| Clippy `-D warnings` in the new module | | **0 lint sites** |

Full-suite delta from the Phase 6.2 baseline: **1984 → 2004**, i.e. exactly the 20 new tests.

Targeted regression suites, all passing:
`qa_memory_risk_tests` 21, `qa_memory_retrieval_tests` 21, `qa_memory_planning_tests` 14,
`qa_memory_perf_tests` 7, `qa_memory_validation_tests` 7, `qa_memory_tests` 25,
`qa_memory_migration_tests` 1, `adaptive_intelligence_tests` 39, `risk_prioritisation` 20 (+1 ign),
`risk_semantics_tests` 10, `risk_signal_tests` 18, `change_impact_tests` 19, `integrity_tests` 34
(+1 ign), `exploration_tests` 29 (+1 ign), `api_tests` 316, `trace_tests` 21, `trace_chain_tests` 14,
`store_tests` 36, `engine_tests` 33, `governed_healing_tests` 53, `agent_tests` 20,
`agent_execution_tests` 33, `retention_and_metrics_tests` 10.

### Previously-flaky duration tests remain stable

The F2 follow-up's two repaired tests were re-run as part of `integrity_tests` ×20 → **20/20**.
Unaffected by this phase, which changed no production code.

---

## 9. Files changed

| File | Nature | Justification |
|---|---|---|
| `services/orchestrator/src/integration_e2e_tests.rs` | **new**, 20 tests | the deliverable: persisted-data E2E coverage |
| `services/orchestrator/src/lib.rs` | **+5 lines** | register the module and document it |

**Production code changed: none.** No migration, route, formula weight, or threshold was added or
altered. The remaining 22 modified files are the already-reported Phase 6.1 + F2 work, untouched
here (verified by mtime: their last write predates this phase).

---

## 10. Remaining integration gaps

Recorded rather than papered over.

1. **No module-change impact path exists.** `resolution_note` marks `module` as refused: *"a module
   vertex exists, but no `covers` edge is ever written"*. So a module change cannot be proven to
   affect any case. S4 asserts the refusal. Building the path is Phase 7+ work, not a gap to fill
   here.
2. **`code_file`, `api_contract`, `page`, `dependency` targets are unresolvable** — no VCS, no
   diff parser, no API/page vertices. S4 pins the refusal and the reason string.
3. **No bug draft is created automatically.** `ingest_run` writes clusters only; a draft needs an
   explicit `insert_bug_draft`. S10b therefore *creates* one through that writer. An
   auto-promotion path from cluster to draft does not exist, and inventing one was out of scope.
4. **No live browser or executor is driven.** Exploration findings in S8 carry `confidence: 0.0`
   and a basis saying no executor evaluated the assertions. Asserting verified assertions would be
   fabricating evidence.
5. **Failure-to-bug works one way only.** A draft reaches the safety floor and `bug_history`; there
   is no reverse path from a draft back to execution telemetry.
6. **Single-node deployment.** The E2E layer runs the SQLite path only. Postgres/multitenancy
   isolation is covered by unit tests, not here.

---

## 11. Open decisions — D3, D5, D6

Not resolved. Each existing contract was validated independently, as instructed.

| | Decision | Status | What Phase 6.2 did |
|---|---|---|---|
| **D3** | Canonical risk formula | **OPEN** | Validated `TestRisk` as its own documented contract. `calibration` is `None` whenever the model is partially measured (S6a, S6b) — asserted, not chosen. **No formula picked, no weights or thresholds changed.** |
| **D5** | 10 ms duration floor | **OPEN** | S7c measures the floor's actual behaviour: governs below the crossover, inclusive at 10.0, exclusive at 10.001, relative above 100 ms, and a 5 000 ms disagreement still disagrees. **This verifies what the constant does; it does not endorse the value as the correct production threshold.** |
| **D6** | `set_task_status` signature | **OPEN** | Validated documented behaviour: the two-flag contract is used by every fixture (S1–S10) and a non-terminal task leaves `finished_at` unset (S9e). **Not redesigned.** |

---

## 12. Verdict

**PHASE 6.2 = PASS.**

PASS gate, item by item:

| Gate | Result |
|---|---|
| real persisted records created through supported writers | **Yes** — every row via a public `Store` method, `ingest_run`, or the HTTP router; no hand-written SQL |
| end-to-end retrieval returns the correct chain | **Yes** — S1 walks the full roadmap chain |
| missing/ambiguous data remains missing/ambiguous | **Yes** — S6b, S9c, S9e, S7b |
| project isolation passes | **Yes** — S5, S5b |
| relevant backend, lint and regression checks pass | **Yes** — 2004/0/5, clippy 0, fmt clean but the one pre-existing diff |
| all discovered bugs fixed and retested | **Yes** — the only bugs found were in the new test code (§6); no production defect was found |

### Recommended next action

**Phase 7 — Production-like User Journey Intelligence**, which is the roadmap's next phase and is
unblocked: no blocker from 6.2 requires a product decision first.

Two things to carry into it:
- The module-change impact path (§10.1) and cluster→bug-draft promotion (§10.3) are the two places
  where a user journey would currently stall on a refusal rather than on evidence.
- **D3 and D6 remain open** and will matter more in Phase 7, where journeys are scored end to end.

---

## 13. Housekeeping

- Phase 7, 8, 9 and 10 were **not** started.
- Nothing was committed or pushed. `HEAD` is still `7930507`, 0 ahead / 0 behind `origin/main`.
- No `reset`, `clean` or `discard`. All 22 modified files and all 4 untracked files from before this
  phase are intact; 2 untracked files were added (this report and the new test module).