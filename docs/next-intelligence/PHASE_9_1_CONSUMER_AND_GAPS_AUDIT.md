# PHASE 9.1 — CONSUMER AND GAPS AUDIT

**Audit and design only. No production code, test behaviour, schema or route was changed. No test
artifact was deleted during this phase.**

Repository: `https://github.com/abhisummer22-design/AIQA.git`
Branch: `main` · HEAD: `7930507` · 0 ahead / 0 behind `origin/main`
Roadmap: `AIQA_NEXT_ROADMAP_PHASED_PLAN.md`

## Verdict

**Phase 9's PASS should be qualified, not withdrawn.** The roadmap's own PASS gate (L360) is met on
its stated terms. But the roadmap lists 13 *required tests* (L344–356) and six were not implemented.
A gate can be met while required tests are unmet; both facts are reported here rather than one
being chosen.

**Recommended qualification: `PASS WITH DEFERRED ACCEPTANCE CRITERIA`** — conditional on the
approvals in §10.

The headline technical finding is in §7: the test-artifact leak has a **precise, source-verified
root cause**, and it is a one-line-shaped fix rather than a cleanup policy.

---

## 1. Initial repository state

| | measured now | reported | match |
|---|---|---|---|
| branch / HEAD | `main` @ `7930507` | same | ✅ |
| ahead / behind | 0 / 0 | 0 / 0 | ✅ |
| modified / untracked / deleted / staged | 25 / 23 / 0 / 0 | 25 / 23 | ✅ |
| `C:` free | 57.83 GB | — | — |
| `H:` free | 424.31 GB | — | — |

**Artifact state, re-measured rather than assumed.** The brief asked me to treat the 55,453 / 63.87 GB
figures as historical. Current measurement:

| | now |
|---|---|
| leftover `.tmp*` / `scoped_dir*` directories | **6,984** |
| total `TEMP` size | **8.01 GB** |

These accumulated *since* the Phase 9 cleanup. The leak is ongoing and reproducible — not a
one-off.

---

## PART A — Reassessing the Phase 9 acceptance gate

### 2. The roadmap requirement

Phase 9 lists **13 required tests** (L344–356): no historical data; all required checks pass;
critical failure; high defect count; unexecuted mandatory test; insufficient requirement coverage;
integrity mismatch; high flakiness; conflicting signals; stale evidence; different project
isolation; same metrics with different missing-data conditions; historical comparison across real
recorded releases.

Phase 9 implemented 7 of 13 scenarios and named 6 as not implemented.

### 3. Does this invalidate PASS?

**No, but it does qualify it.** Two separate things are being conflated:

* The **PASS gate** (L360) reads: *"Release recommendation must be explainable and project-scoped,
  recognize missing or stale evidence, follow explicit release gates, and pass regression tests."*
  Every clause of that is met, and mutation-verified.
* The **Required tests** list (L344–356) is a separate, more specific commitment. Six entries are
  unmet.

A phase can satisfy a gate while missing required work. **The Phase 9 report should have said so in
its verdict rather than reporting an unqualified PASS**, and this audit records the correction.

### 4. Analysis of the six

Facts below are from source. "Source exists?" is the decisive column.

| # | test (L) | required evidence | source exists? | recommendation |
|---|---|---|---|---|
| 1 | high defect count (L347) | bug drafts attributable to **this release**, with severity | **Partial.** `bug_drafts` exists with `review_state`, read by `Store::list_bug_drafts(project_filter)` — but **no `release_id` column** (verified in `0002_*.sql`) | **DEFER** — needs Part C schema work first |
| 2 | insufficient requirement coverage (L349) | requirements vs. test cases, **per release** | **Yes, at case level.** `requirement_history(project, requirement_key)`, `list_test_cases_for_requirement`, and Phase 8's `requirement_linkage` all exist | **IMPLEMENT** — no migration needed |
| 3 | integrity mismatch (L350) | a run of this release with an established integrity defect | **Yes.** Already implemented as gate **R7** in Phase 9; the *scenario* exists (`p9_read_a_release_with_a_product_failure_is_no_go` exercises the sibling path). What is missing is a fixture that produces a *real* integrity defect | **IMPLEMENT** — needs a defect-producing fixture, not new logic |
| 4 | high flakiness (L351) | flakiness above the established band, for this release | **Yes.** `qa_memory_risk::FLAKY_THRESHOLD` exists, and `release_risk` carries a `flakiness_score` factor and a `flakiness` signal | **IMPLEMENT** — no migration, no new threshold |
| 5 | conflicting signals (L352) | two signals disagreeing | **Yes, derivable.** A release with a low completed-failure rate *and* a blocked mandatory test, or infra errors *and* a clean pass rate, is a genuine conflict | **IMPLEMENT** — but only as an explicitly-labelled *finding*; the roadmap defines no conflict-resolution rule, so it must not silently pick a winner |
| 6 | stale evidence (L353) | evidence old enough to not reflect the release | **Partial.** `ReleaseRisk::window` gives real bounds and Phase 9 reports them — but the roadmap defines **no staleness threshold**, so nothing can judge "stale" | **CLARIFY ROADMAP FIRST** — inventing a threshold is exactly what rule 1 forbids |

**Summary: 4 implementable now, 1 needs Part C first, 1 needs a roadmap clarification.**

Two of these deserve emphasis:

* **#4 (high flakiness) was arguably already satisfiable in Phase 9** and was not implemented. It
  is the clearest genuine gap in the Phase 9 report.
* **#6 (stale evidence) cannot be implemented honestly today.** The roadmap requires a test for
  staleness but defines no staleness criterion anywhere. Implementing one means inventing a
  threshold. That is a decision for the roadmap owner, not an implementation choice.

---

## PART B — The production consumer

### 5. Established conventions (from source, not invented)

| convention | evidence |
|---|---|
| route shape | `/api/v1/projects/:project_id/intelligence/<thing>` — 14 such routes exist (`tests`, `telemetry`, `failures`, `flakiness`, `risk`, `strategies`, `trends`, `quality-graph`, `investigations`, `retention/*`) |
| isolation gate | `require_scoped_project(...)` — **91 call sites**; `require_role(...)` — 4 |
| handler shape | `async fn(state: State<AppState>, Extension(ctx): Extension<AuthContext>, ApiPath(project_id): ApiPath<Uuid>) -> Result<Json<…>, ApiError>` |
| error mapping | `ApiError::internal` for store faults (the Phase 7.1 contract: propagate, never mask) |
| serialization | `serde::Serialize` on view structs; `serde_json::json!` insertion for additive keys |

**Critical finding: there is no release-scoped HTTP route in this codebase at all.** `release_risk`,
`release_history`, `release_risk_overview` and `test_quality` are all internal readers with no route.
So there is no existing release-route convention to copy — only the project-route convention.

### 6. Option comparison

| | **Option 1** internal service | **Option 2** read-only API | **Option 3** roadmap alternative |
|---|---|---|---|
| Application changes | none | 2 handlers + 2 route registrations + OpenAPI/rbac-probe entries | Phase 10 only |
| Authorization | n/a (caller's responsibility) | `require_scoped_project`; release id verified **in-project** before use | inherited |
| Request identifiers | caller-chosen | `project_id` (path) + `release_id` or `test_case_id` | varies |
| Isolation | caller's scope | path project + in-project release membership check | — |
| Error propagation | `anyhow::Result` | map to `ApiError::internal`, never to `200` | — |
| Missing evidence | `missing_signals` in the struct | same, serialized | — |
| Compatibility | none to preserve | additive only; nothing existing changes | — |
| Tests needed | existing suites suffice | new auth/isolation/route tests, plus RBAC probe entry | — |
| Advantage | smallest; no public surface | makes both assessments usable | aligns with roadmap intent |
| Disadvantage | **nothing consumes it** — the problem stays | adds public API needing maintenance | later, larger |

### 7. Recommendation: **Option 1 now, Option 2 gated on approval**

Phase 9.1 must not implement either. The recommendation is staged:

* **Option 1 is the minimum that satisfies the roadmap today.** Neither the Phase 9 Output block nor
  the Phase 10 architecture section requires a public endpoint; Phase 10 L390 explicitly says
  *"Retrieve evidence using existing project-scoped APIs/services"* — **services are named first**.
* **Option 2 is the right end state**, but a route is public API surface and an authorization
  decision. It needs approval, and it needs `rbac_probe.rs` and the OpenAPI surface updated to stay
  consistent with this repository's conventions.

**The honest observation:** adding Option 1's consumer without a route means adding a caller that
does not exist yet. Its value is that it forces the contract to be exercised from a service boundary
rather than only from tests. That is real but modest. **Phase 10 is the natural first consumer.**

---

## PART C — Release-scoped evidence gaps

### 8. Verification that no release attribution exists today

Every table was checked in its defining migration:

| table | migration | `release_id`? |
|---|---|---|
| `failure_clusters` | `0002_quality_graph_and_intelligence.sql` | **NO** |
| `bug_drafts` | `0002_quality_graph_and_intelligence.sql` | **NO** |
| `change_impacts` | `0002_quality_graph_and_intelligence.sql` | **NO** |
| `exploration_findings` | `0032_exploration.sql` | **NO** |
| `exploration_sessions` | `0032_exploration.sql` | **NO** |
| `flakiness_metrics` | `0002_quality_graph_and_intelligence.sql` | **NO** |

`runs.release_id` exists (migration 0029). That is the **only** release linkage in the schema.

### 9. Per-signal assessment

| signal | can it be attributed to a release today? | what is missing |
|---|---|---|
| **defect severity** | **No.** `bug_drafts` → `failure_clusters` → `quality_nodes` → `test_cases`. A defect reaches a *case*, never a release | `bug_drafts.release_id`, or a defensible case→release rule |
| **failure clusters** | **No.** Same chain; clusters are per-project with a signature | `failure_clusters.release_id` or a run linkage |
| **change impact** | **No.** `change_impacts` → `quality_nodes` | same |
| **exploratory findings** | **No.** `exploration_sessions` has `target` (free text), no release | `exploration_sessions.release_id` |
| **journey outcomes** | **Not applicable.** Journeys are executable tests in `journey_tests.rs`, not persisted records. There is nothing to scope | a persisted journey-run entity, or accept that journeys are a test-suite property |

### 10. Minimum change if release scoping is wanted

**Not proposed for implementation — recorded for the approval decision.**

The smallest defensible design is **one nullable `release_id` on each of the five tables**, matching
the existing `runs.release_id` convention exactly: nullable, never auto-populated, no backfill
(0029's own stated pattern). That preserves "answer `INSUFFICIENT_DATA` rather than invent an
association".

**A cheaper alternative exists and should be considered first:** attribute nothing, and report the
five signals as **project-scoped context** on the readiness result — which is what Phase 9 already
does, with `EvidenceScope::Project` and `NO_SOURCE_IN_SCHEMA`. This is honest, requires no migration,
and the roadmap's own rule 7 ("do not invent … historical releases") arguably prefers it.

**Recommendation: take the cheaper alternative. Add release columns only when a consumer genuinely
needs per-release attribution** — which is a Phase 10 question, not a Phase 9.1 one.

---

## PART D — Mandatory regression requirements

### 11. What actually exists

Three distinct concepts, checked in source:

| concept | exists? | where |
|---|---|---|
| **A. Explicitly declared mandatory tests** | **NO** | No column, tag convention, or field anywhere. `qa_memory_risk` states this in its own module docs |
| **B. A derived regression safety floor** | **YES** | `PriorityBand::Mandatory`, derived as "a test case with recorded failure evidence"; surfaced as `TestPrioritisation::mandatory_floor` |
| **C. Impact-based prioritisation** | **YES** | `test_prioritisation`, ordering by measured risk |

**B and C are currently distinguishable** — `mandatory_floor` is a separate field from the ordered
list, and `Scope::restricted_to` is what enforces retention when a run is scoped down. **A is
absent entirely.**

### 12. What the roadmap expects the rule to protect

Rule 3: *"Do not classify a release as safe solely because tests passed when required tests were
not run."* Rule 5: *"Preserve explicit business/user-defined release gates."*

Rule 5 presupposes **A** exists. It does not. So there is currently **no explicit business-defined
gate to preserve** — the requirement is unsatisfiable as written against this schema.

### 13. Recommendation

**Preserve the derived floor (B), and describe it explicitly as a derived safeguard.** Do not
introduce a flag in this phase.

Phase 9's R5 gate already does this and reports it honestly: when the derived floor is empty it
emits `regression.mandatory_floor` with `NO_SOURCE_IN_SCHEMA` and the reason *"there is NO declared
mandatory-test flag anywhere in this schema"*. A test asserts exactly that wording.

**Introducing authoritative mandatory-test policy requires a separately approved schema change** —
a new column or table, a writer, an API surface, and a backfill decision. That is a product
decision about what *shall* be mandatory, not an engineering task.

**Clarification needed from the roadmap owner:** does rule 5 mean "preserve gates if any exist" (the
current state is then compliant), or "mandatory-test policy must exist" (it is then unmet)? These
have opposite answers.

---

## PART E — Test-artifact disk accumulation

### 14. Root cause (established from source)

Four verified facts compose into one mechanism:

1. **`Store::connect` enables WAL.** `store.rs` sets
   `.journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)`. WAL creates `*.sqlite3-wal` and
   `*.sqlite3-shm` sidecars, held open for the life of the connection.
2. **`Store` has no `Drop` impl and never closes its pool.** Verified: `impl Drop for Store` → not
   present; `pool.close()` → **0 occurrences** in `store.rs`.
3. **Test fixtures bind the temp dir *after* the store.** The dominant pattern is
   `let (state, _dir) = test_state().await;` — **603 bindings across 25 test files**.
4. **Rust drops locals in reverse declaration order.** So `_dir` drops **first**, calling
   `remove_dir_all` while `state` — and its open SQLite pool — is still alive.

On Windows, removing a directory whose files are open **fails**. `tempfile` swallows that error. The
directory and its `.sqlite3` + `-wal` + `-shm` files leak.

**The evidence matches exactly.** Every inspected leaked directory contains precisely a
`*.sqlite3` file plus its `-wal`/`-shm` pair (or a bare `prune.sqlite3` where WAL was never
checkpointed) — 114 × `api.sqlite3`, 77 × `phase6-integrity.sqlite3`, 21 × `test.sqlite3`, etc.

**Answers to the brief's specific questions:**

| question | answer |
|---|---|
| Which test code creates them | Any test using `tempfile::tempdir()` + `Store::connect`. 603 binding sites across 25 files |
| Naming/location | `.tmpXXXXXX` under `std::env::temp_dir()` = `%TEMP%` = `C:\Users\Abhi\AppData\Local\Temp`; `scoped_dir*` are sqlx migration dirs |
| Why they persist | Reverse drop order + open WAL handles + swallowed Windows removal error (§14.4) |
| Required for debugging? | **No.** They are discarded SQLite databases; the tests assert on in-memory values, not on the file |
| Concurrency risk? | **Yes, if fixed carelessly** — see §15 |
| Does cleanup exist? | **No.** `tempfile`'s is attempted but fails silently on Windows |

### 15. Smallest proposed fix

**Give `Store` a `Drop` impl that closes the pool, and declare the temp dir before the store so
drop order is correct.**

Two options, both small:

* **(a) Preferred — `impl Drop for Store { pool.close() }`.** Closing the pool releases the file
  handles regardless of drop order, so `remove_dir_all` then succeeds. One impl, fixes all 603 sites,
  and is correct regardless of how a caller binds things.
* **(b) Alternative — bind `dir` before `state` in fixtures** (`let (dir, state) = …`). Fixes drop
  order but not the missing pool close, and requires touching 25 files.

**This audit proposes (a) and implements nothing.** It is a production-code change requiring approval.

**Why a cleanup script is the wrong fix**, per the brief's criteria 1–7:

1. It cannot distinguish *this* run's artifacts from another concurrent run's — every name is random.
2. A blanket `Remove-Item $env:TEMP\.tmp*` can delete a **concurrently running** test's live database.
3. It cannot tell a user file from a test artifact.
4. It is exactly the "manually deleting unknown directories" the brief rules out.

**Concurrency is a real constraint on any fix.** `cargo test` runs tests in parallel by default, so
many `.tmpXXXXXX` directories are live simultaneously. Any cleanup must therefore either run
*between* test runs (never during), or key on ownership the directory name does not carry. The
`Drop`-based fix sidesteps this entirely because each directory is removed by its own creator.

**Validation plan for the fix:** run the full suite; assert `TEMP` artifact count returns to
pre-run baseline; run two full suites concurrently to prove no cross-deletion; confirm no test needs
the on-disk file after its store is dropped; confirm the whole suite stays green.

---

## CORRECTION NOTICE — §14 and §15 superseded

> **Status: §14 and §15 are WITHDRAWN. Sections 14 and 15 above are retained verbatim as the
> original investigation and are deliberately not rewritten. This notice supersedes them; where
> they disagree with this notice, this notice is correct.**
>
> **Issued:** during Phase 9.2 / 9.2A, after the §15 option (a) recommendation was empirically
> tested and **disproven**. The error was caught only by measurement, not by review.

### C.1 What was wrong

§15 preferred **`impl Drop for Store { pool.close() }`** and offered **binding `dir` before `state`**
as the alternative. Both rest on the assumption that `Pool::close()` releases the file handles on
its own. Measured on this repository, it does not.

### C.2 The SQLx API fact that breaks option (a)

Verified against the pinned dependency, `sqlx-core-0.9.0`, `src/pool/mod.rs:441`:

```rust
pub fn close(&self) -> impl Future<Output = ()> + '_
```

`close` is **not** an `async fn`; it **returns a Future that the caller must await**. `Drop` cannot
await. So `impl Drop for Store { self.pool.close(); }` constructs a `Future` and **immediately drops
it, unpolled and never awaited**. It is a silent no-op — it compiles, produces no warning, and
releases nothing. The sqlx documentation's claim that dropping the pool releases connections (docs
L436–438) does not hold for SQLite on Windows.

### C.3 Measurements that disprove the original hypothesis

Test fixtures were created, used, and torn down under three orderings/leavings, counting
directories in `%TEMP%` directly:

| variant | directories leaked |
|---|---|
| Current fixture order (store bound first, `_dir` dropped first) | **5 / 5** |
| Corrected drop order only (`dir` bound before `store`) | **5 / 5** |
| Corrected drop order **+ awaited** `pool.close().await` | **0 / 5** |

**Changing drop order alone does not fix the leak.** Only an *awaited* `pool.close()` does. Option
(b) is therefore not a fix, and option (a) as written cannot be made to work.

### C.4 Corrected root cause

The leak is caused by the **lifetime of open SQLite pool connections when the temporary directory is
removed** — not by the order in which two locals are declared. Both halves are required:

* the pool must be **closed**, because only an awaited close releases the underlying file handles
  before `remove_dir_all` runs; and
* the close must **complete before** the `TempDir` is dropped.

`tempfile` still swallows the Windows removal error, so a missing close is silent — which is why this
survived review and only measurement caught it.

### C.5 Phase 9.2A confirmation on a live suite

`integrity_tests::harness` (29 call sites) was remediated with an awaited `store.pool.close().await`
at the end of each test. Measured, interleaved with unpatched control suites so scheduling order
cannot explain the result:

| suite | helper | `.tmp` delta per run |
|---|---|---|
| `integrity_tests` | `harness` (patched) | **0** (5 consecutive runs) |
| `qa_memory_retrieval_tests` | `temp_store` (control) | +21 |
| `qa_memory_tests` | `temp_store` (control) | +25 |
| `exploration_tests` | `harness` (control) | +18 |

Neuter **all 34** patched closes in `integrity_tests` and the leak returns at **+28**. This is
consistent and precise: of the 29 harness-using tests, 28 genuinely open a handle and leak, and 1
(`a_run_the_layers_agree_about_is_recognised_as_consistent`, 0 database operations) never opens one
and so never leaked even before this phase.

### C.6 Remaining limitations of this correction

* The +28/+29 figure is measured for `integrity_tests` only. **It is not a repository-wide total.**
* The other ~700 fixture call sites across ~39 helpers are **still unmodified and still leaking**;
  their per-suite deltas are only spot-measured (see the table above).
* The exact Windows mechanism by which an unclosed SQLite pool keeps the file undeletable is
  **not established**. The behaviour is measured and reproducible; the underlying file-lock detail
  is not explained.
* Whether an unclosed pool also explains the one unexplained earlier full-suite failure remains
  **unproven**. The disk-pressure magnitude (~1.1 GB per full run) is measured; causation is not.

### C.7 What replaces §15

Not a production `Store::close()`. The working pattern is an **awaited** `store.pool.close().await`
at the end of each test, before its `TempDir` drops. This is test-only, but it is per-call-site and
therefore scales with the number of call sites rather than with one `impl` block. The cost/benefit
and the production-API alternative are analysed in
`PHASE_9_2_TEST_ARTIFACT_LIFECYCLE_REPORT.md` and `PHASE_9_2A_FIXTURE_LIFECYCLE_REPORT.md`.

---

## PART F — Phase 10 dependencies

Read from L369–423. The eight example questions map onto the audit findings:

| Phase 10 question (L) | needs |
|---|---|
| "What is blocking release readiness?" (L384) | **Phase 9 consumer** — mandatory |
| "Why is this test high risk?" (L380) | `release_risk`/`test_risk` readers — exist |
| "Which requirements lack test coverage?" (L382) | **Phase 8 quality consumer** — mandatory |
| "Why did this release fail?" (L377) | release-scoped failure attribution — **gap** |
| "Which changes affect my regression suite?" (L379) | change-impact — **gap (Part C)** |
| "Which failures are recurring?" (L381) | failure clusters — **gap (Part C)** |
| "Why was this finding classified as insufficient evidence?" (L383) | both assessments' `missing_signals` — available |
| "What should I test next?" (L378) | prioritisation — exists |

### 16. Mandatory vs. deferrable

| dependency | mandatory? |
|---|---|
| Production consumer for `TestCaseQuality` | **Yes** for L382/L383. Can be satisfied by **Option 1** (Phase 10 retrieves through services, per L390) — no route strictly required |
| Production consumer for release readiness | **Yes** for L384. Same — Option 1 suffices |
| Release-scoped signal attribution | **No** — Phase 10 can answer honestly with project-scoped evidence plus an explicit "not attributable to a release" statement, which rule 6 ("do not hallucinate") actually requires |
| Explicit mandatory-test policy | **No** — Phase 10 can cite the derived floor and say it is derived |
| Canonical risk/severity formula (D3) | **No** — Phase 10 can report both existing severities with their provenance, as Phase 9 does. Answering "why is this test high risk" does not require resolving which severity algorithm is canonical |

**D3, D5 and D6 remain open.** This audit resolves none of them.

---

## 17. Minimum proposed changes and their tests

Nothing here is implemented. All require approval.

| # | change | type | tests required | risk |
|---|---|---|---|---|
| 1 | `impl Drop for Store { pool.close() }` | production, ~5 lines | full suite; TEMP artifact count returns to baseline; two concurrent suites | low; touches a core type's lifecycle |
| 2 | Phase 9 consumer service (Option 1) | production | existing suite + one consumer test | low |
| 3 | Phase 8 quality consumer service (Option 1) | production | as above | low |
| 4 | Readiness tests #2 coverage, #3 fixture, #4 flakiness, #5 conflicting signals | tests only | the tests themselves | low |
| 5 | Read-only routes (Option 2) | **API** | auth, isolation, RBAC probe, OpenAPI, route tests | **medium** — public surface |

**Explicitly not proposed:** any migration, any new flag, any staleness threshold, any aggregate
score, any canonical formula.

---

## 18. Decisions requiring approval

| # | decision | why it needs you |
|---|---|---|
| D-1 | Qualify Phase 9 as `PASS WITH DEFERRED ACCEPTANCE CRITERIA` | Six roadmap-required tests are unmet; only you can accept that |
| D-2 | Staleness criterion for test #6 | The roadmap requires the test but defines no threshold. Inventing one violates rule 1 |
| D-3 | Option 1 vs Option 2 for the consumer | Option 2 is public API surface with authorization implications |
| D-4 | Route naming, if Option 2 | No release-route convention exists to copy; I will not invent one unprompted |
| D-5 | Release-scoping: no migration, vs nullable `release_id` × 5 | A schema decision with migration and backfill consequences |
| D-6 | Rule 5 interpretation (§13) | "Preserve gates if any exist" and "policy must exist" have opposite answers |
| D-7 | `impl Drop for Store` | Core lifecycle change to a widely-used type, for test hygiene |
| D-8 | Whether `PHASE_7_PASS.md` / `PHASE_8_PASS.md` are still owed | Both were roadmap deliverables those phases' reports deferred |

---

## 19. Ordered implementation plan

Each step states its acceptance criterion. **Do not begin without approval of the preceding step.**

**Step 0 — Qualification (no code).**
Accept `PASS WITH DEFERRED ACCEPTANCE CRITERIA` for Phase 9, or reject and reopen it.
*Accept:* the six unimplemented tests are recorded as deferred, not complete.

**Step 1 — Stop the disk leak.** Approve D-7.
*Accept:* `impl Drop for Store` closes the pool; one full suite leaves zero new `.tmp*` directories;
two concurrent full suites both stay green and neither deletes the other's artifacts.

**Step 2 — Readiness tests #2, #3, #4, #5.** No approval needed beyond Step 0; tests only.
*Accept:* each of the four roadmap scenarios has a deterministic test that fails if its gate is
removed; four mutations applied, caught, restored byte-identical.

**Step 3 — Consumer, Option 1.** Approve D-3 as Option 1.
*Accept:* a supported service path returns both assessments; project and release isolation hold;
a failed required read propagates; no route is added.

**Step 4 — Staleness (#6).** Approve D-2, or accept formal deferral.
*Accept:* either a roadmap-supplied criterion with a test, or a recorded deferral with the reason.

**Step 5 — Option 2 routes.** Approve D-3 as Option 2 and D-4.
*Accept:* additive routes only; `require_scoped_project` on both; cross-project release ids rejected;
`rbac_probe.rs` and OpenAPI updated; existing clients unaffected.

**Step 6 — Release scoping.** Approve D-5. Not required for Phase 10.
*Accept:* either per-release attribution exists with a migration, or the five signals stay
project-scoped and are labelled as such.

**Not in this plan:** Phase 10 itself, and resolutions of D3/D5/D6.

---

## 20. Facts versus recommendations

**Established from source code (verifiable):**

* WAL is enabled on connect; `Store` has no `Drop`; `pool.close()` appears 0 times in `store.rs`.
* 603 `let (state, _dir)`-style bindings across 25 test files; Rust's reverse drop order removes the
  temp dir first.
* Every leaked directory inspected contains a `*.sqlite3` plus `-wal`/`-shm` sidecars.
* None of `failure_clusters`, `bug_drafts`, `change_impacts`, `exploration_findings`,
  `exploration_sessions`, `flakiness_metrics` carries a `release_id`.
* No release-scoped HTTP route exists; 14 `/intelligence/*` routes exist, gated by
  `require_scoped_project` (91 call sites).
* `FLAKY_THRESHOLD`, `PriorityBand::Mandatory`, `mandatory_floor`, `bug_drafts.review_state`,
  `requirement_history`, and Phase 8's eight quality dimensions all exist.
* No declared mandatory-test flag exists anywhere in the schema.
* The roadmap defines four readiness states and lists 13 required tests; 6 were not implemented.

**Recommendations (mine, arguable):** the qualification wording; Option 1 over Option 2; the
no-migration path for Part C; `impl Drop for Store` as the fix; implementing four of the six tests.

**Unresolved assumptions I could not verify:**

* Whether the roadmap's staleness test implies a threshold exists elsewhere in the product spec —
  it does not appear in this roadmap file.
* Whether rule 5 presupposes a mandatory-policy feature or is conditional on one existing.
* Whether a stale-evidence judgment is wanted at all, given rule 6's bar on ungrounded answers.
* Whether the unexplained earlier full-suite failure was disk-related. **The mechanism is now real
  and quantified; the specific failure is still unexplained, and I do not claim otherwise.**

---

## Appendix — safety statement

Nothing was modified in this phase: no production code, test behaviour, schema, or route; no commit;
no push; no artifact deleted. Working tree unchanged at 25 modified / 23 untracked, HEAD `7930507`
0/0. All 23 untracked files, including every prior phase report, are intact.