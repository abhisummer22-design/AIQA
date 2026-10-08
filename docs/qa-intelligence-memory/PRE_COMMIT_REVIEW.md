# Pre-Commit Review — QA Intelligence Memory

**Review date:** 2026-10-07 · **B1 cleared:** 2026-10-08
**Branch:** `main` (HEAD `2a4b668`, up to date with `origin/main`)
**Scope:** all six phases of `AIQA_QA_INTELLIGENCE_MEMORY_MASTER_PLAN.md`
**Reviewed:** 44 files total — 16 modified application/source, 12 new
application/source, 16 new documentation. 0 modified documentation, 0 unrelated.
7 test suites, 3 migrations.

---

## 0. Verdict

## ✅ **GO**

The code is sound, safe and fully tested. Every file belongs to the QA Intelligence
Memory work, all three migrations are additive and verified against a copy of the live
database, project isolation holds on every new read and write path, and nothing new
persists a secret.

The one **process gap** found by this review — the missing phase reports for phases 3, 4
and 5 — has been **cleared**. It was cleared by writing the documents, **not** by
changing any application logic, weakening any test, or relaxing any claim.

Two cosmetic defects were found and fixed during the review (see §6). Neither affected
behaviour.

### B1 resolution record

| | |
|---|---|
| **Finding** | Master plan rules #7 and #8 require a phase report and a `PHASE_<N>_PASS.md` per phase. Phases 3, 4 and 5 reported PASS in conversation with 53 tests across three suites, but the artefacts were never written to disk. `PHASE_1_REPORT.md` was also absent (only the audit existed). |
| **Resolution** | Seven documents written: `PHASE_1_REPORT.md`, `PHASE_3_REPORT.md`, `PHASE_3_PASS.md`, `PHASE_4_REPORT.md`, `PHASE_4_PASS.md`, `PHASE_5_REPORT.md`, `PHASE_5_PASS.md`. `FINAL_VALIDATION_REPORT.md` §4 gained a documentation index and the exact per-category file counts. |
| **Method** | Every figure in the new documents was re-measured, not recalled: per-phase test counts were run individually (25 / 21 / 14 / 18 / 7 / 7 / 1 = 93), and the code-level facts they cite — function line numbers, file line counts, the store methods added — were read back out of the source. |
| **Application logic changed?** | **No.** The only source edits in the whole review were the two cosmetic fixes in §6 (R1, R2), both made **before** this finding was raised. Clearing B1 touched `docs/` exclusively. See §0.1 for the exact reconciliation, because "0 source files" and "28 source files" are both true of different things. |
| **Tests weakened or skipped?** | **No.** No test file was edited while clearing B1. The suite is still 1809 passed / 0 failed / 2 ignored, verified after the documents were written. |
| **Honest caveat** | The reports are a *record written after the fact*, not a contemporaneous one. The work and its test evidence are unchanged and were never in doubt; what was late was the written record. That is what B1 was, and it is stated in `FINAL_VALIDATION_REPORT.md` §4 rather than papered over. |

### 0.1 Reconciliation — "0 source files" versus "28 source files"

Both numbers appear in this review's history and they are not in conflict, but the
unqualified form of either is misleading, so the distinction is stated here.

| Question | Answer |
|---|---|
| How many application/source files are **in the change**? | **28** — 16 modified + 12 new |
| How many application/source files were edited **while clearing B1**? | **0** |
| How many executable files were touched at any point during the review? | **2**, both cosmetic (R1, R2) and both pre-B1 |

The confusion arose from a conversational summary that said "Source files: 0"
immediately before "16 modified, 12 new". Read alone, the first number implies no source
file is involved in the change; it actually means *no source file was touched while
writing the missing documents*, which is what B1 required. That is what the
**Application logic changed?** row above records, and this table removes the ambiguity.

### 0.2 Exact counts, by the command that produces them

Derived from `git diff --name-only` and `git ls-files --others --exclude-standard`:

| Category | Modified | New | Total |
|---|---|---|---|
| Application / source | 16 | 12 | **28** |
| Documentation | 0 | 16 | **16** |
| Unrelated | 0 | 0 | **0** |
| **Total** | **16** | **28** | **44** |

Of the 12 new application/source files: **3 migrations** and **9 Rust files**
(2 implementation + 7 test modules).

Of the 16 new documentation files: **15** under `docs/qa-intelligence-memory/` and
**1** at the repository root, `AIQA_QA_INTELLIGENCE_MEMORY_MASTER_PLAN.md` — the
user-authored specification this plan implements, which belongs in the repository so
the reports' references to it resolve.

**Unrelated files: none.** Every path is either the plan's implementation, its tests,
its migrations, or its written record. `git status --short` reports 30 lines rather than
44 because it collapses the untracked `docs/qa-intelligence-memory/` directory into a
single entry; `git ls-files --others` enumerates the 15 files individually.

---

## 1. Files changed — complete manifest

### 1.1 New migrations (3)

| File | Statements |
|---|---|
| `services/orchestrator/migrations/0028_qa_memory_linkage.sql` | `ALTER TABLE tasks ADD COLUMN`; 2 `CREATE INDEX IF NOT EXISTS` |
| `services/orchestrator/migrations/0029_qa_releases.sql` | `CREATE TABLE IF NOT EXISTS`; `CREATE UNIQUE INDEX IF NOT EXISTS`; `ALTER TABLE runs ADD COLUMN`; `CREATE INDEX` |
| `services/orchestrator/migrations/0030_failure_cluster_fingerprint.sql` | `ALTER TABLE failure_clusters ADD COLUMN`; `CREATE INDEX IF NOT EXISTS` |

### 1.2 New source (9)

| File | Phase | Bytes |
|---|---|---|
| `src/qa_memory.rs` | 3, 4 | 57,622 |
| `src/qa_memory_risk.rs` | 5 | 33,520 |
| `src/qa_memory_tests.rs` | 2 | 24,418 |
| `src/qa_memory_migration_tests.rs` | 2 | 6,424 |
| `src/qa_memory_retrieval_tests.rs` | 3 | 36,755 |
| `src/qa_memory_planning_tests.rs` | 4 | 21,713 |
| `src/qa_memory_risk_tests.rs` | 5 | 28,492 |
| `src/qa_memory_validation_tests.rs` | 6 | 37,216 |
| `src/qa_memory_perf_tests.rs` | 6 | 17,879 |

### 1.3 Modified source (16) — every one verified on-topic

| File | Change | Belongs to |
|---|---|---|
| `src/domain.rs` | `TaskDef.test_case_id`, `TaskState.test_case_id` | Phase 2 |
| `src/store.rs` | `ReleaseRow`; releases CRUD; module vertices; `backfill_task_test_case_links`; `insert_run_for_release`; `FailureClusterRow.failure_fingerprint`; column plumbing | 2, 6 |
| `src/intelligence.rs` | real case id into telemetry; fingerprint into clusters | 2, 6 |
| `src/api.rs` | handoff links a case; crawl explicitly does not | Phase 2 |
| `src/execution_plan.rs` | `materialize_task_def` forwards `plan.test_case_id` (both branches) | Phase 2 |
| `src/plan_validator.rs` | simulated / hand-declared executors explicitly unlinked | Phase 2 |
| `src/scheduler.rs` | carries the field into the in-memory task | Phase 2 |
| `src/adaptive_intelligence.rs` | `QaIntelligenceFeedback.planning_signals` | Phase 4 |
| `src/operations.rs` | synthesized cluster row carries its fingerprint | Phase 6 |
| `src/executor.rs`, `src/regression.rs` | `test_case_id: None` in test helpers | Phase 2 |
| `src/lib.rs` | registers 2 modules + 7 test modules | 2–6 |
| `src/agents/tests.rs`, `src/api_tests.rs`, `src/store_tests.rs`, `src/operations_tests.rs` | **argument-only** (`None` added) | 6 |

### 1.4 Documentation — new (16)

Fifteen under `docs/qa-intelligence-memory/`:

```text
PHASE_1_AUDIT.md    PHASE_1_REPORT.md   PHASE_1_PASS.md
PHASE_2_REPORT.md   PHASE_2_PASS.md
PHASE_3_REPORT.md   PHASE_3_PASS.md
PHASE_4_REPORT.md   PHASE_4_PASS.md
PHASE_5_REPORT.md   PHASE_5_PASS.md
PHASE_6_REPORT.md   PHASE_6_PASS.md
FINAL_VALIDATION_REPORT.md            PRE_COMMIT_REVIEW.md
```

Plus one at the repository root: `AIQA_QA_INTELLIGENCE_MEMORY_MASTER_PLAN.md`, the
user-authored specification this plan implements.

### 1.5 Documentation — modified (0)

**None.** Every document in this change is new. The only file this review *edited*
after its own creation is `PRE_COMMIT_REVIEW.md`, which was itself untracked, so it
appears in 1.4 rather than here.

### 1.6 Unrelated files (0)

**None.** Every path in `git status --short` is either this plan's implementation, its
tests, its migrations, or its written record. Verified against the full diff: no
unrelated refactors, no formatting churn, no vendored or generated content.

**Totals:** `+575 / −43` in tracked files, all additive.

This figure is the **post-review** count. It was `+576 / −43` when the diff was first
inspected; the difference of one line is cosmetic fix **R2** (§6), which removed a dead
`let _ = row;` and the unused binding it existed to silence.

---

## 2. Ownership — every change belongs to this work

Verified by reading the full diff, not the diffstat:

- **No unrelated refactors.** Every hunk is either a new field, a new function, a new
  column in an INSERT/SELECT, a new argument at an existing call site, or a doc comment
  naming its phase.
- **No existing behaviour changed.** `insert_run` keeps its exact signature and
  delegates to a new sibling with `release_id: None`.
- **No test assertion was weakened, skipped or disabled.** The four existing test files
  touched received **only** an added `None` argument. Confirmed line by line:
  `api_tests.rs` 1 call, `store_tests.rs` 3 calls, `operations_tests.rs` 12 calls,
  `intelligence.rs` 3 calls — all `.upsert_failure_cluster(..., None)`.
- **No frontend file touched.** The plan is backend + docs; frontend is a pure
  regression check here.

---

## 3. Migrations 0028–0030 — safety verdict

### 3.1 Destructive-statement scan, all 30 migrations

```text
DROP          — 0 occurrences
DELETE FROM   — 0 occurrences
ALTER … RENAME — 0 occurrences
UPDATE        — 0 occurrences (in any .sql migration file)
```

**Every migration in the repository is additive.** No migration has ever dropped,
deleted, renamed or rewritten a row.

### 3.2 Statement-level review

| Migration | Statement | Destructive? |
|---|---|---|
| 0028 | `ALTER TABLE tasks ADD COLUMN test_case_id TEXT` | No — nullable column, SQLite preserves all rows |
| 0028 | `CREATE INDEX IF NOT EXISTS idx_tasks_test_case` | No |
| 0028 | `CREATE INDEX IF NOT EXISTS idx_telemetry_test_case` | No |
| 0029 | `CREATE TABLE IF NOT EXISTS qa_releases` | No — new table |
| 0029 | `CREATE UNIQUE INDEX IF NOT EXISTS idx_releases_project_label` | No |
| 0029 | `CREATE INDEX IF NOT EXISTS idx_releases_project_created` | No |
| 0029 | `ALTER TABLE runs ADD COLUMN release_id TEXT` | No — nullable |
| 0029 | `CREATE INDEX idx_runs_release` | No |
| 0030 | `ALTER TABLE failure_clusters ADD COLUMN failure_fingerprint TEXT` | No — nullable |
| 0030 | `CREATE INDEX IF NOT EXISTS idx_clusters_fingerprint` | No |

All three `ALTER TABLE`s add **nullable** columns with no `NOT NULL` and no default, so
every pre-existing row reads `NULL` — the honest "not known" rather than a fabricated
value.

### 3.3 Empirical verification — a copy of the live database

The live file was **copied, never migrated in place**.

```text
BEFORE:  27 migrations · 40 tables
  projects 2 · runs 2 · tasks 2 · execution_telemetry 19 · test_cases 2
  failure_clusters 0 · audit_log 1036 · evidence 2
  tasks.test_case_id            absent
  runs.release_id              absent
  clusters.failure_fingerprint  absent
  qa_releases                   absent

AFTER:   30 migrations · 41 tables
  projects 2 · runs 2 · tasks 2 · execution_telemetry 19 · test_cases 2
  failure_clusters 0 · audit_log 1036 · evidence 2     ← ALL UNCHANGED
  tasks.test_case_id            present
  runs.release_id              present
  clusters.failure_fingerprint  present
  qa_releases                   present, 0 rows
  historical telemetry given an invented link:  0

VERDICT: NO ROW LOST OR CHANGED
```

`qa_releases` is empty after migration — **nothing was fabricated**. Historical telemetry
gained **zero** links, which is correct: pre-Phase-2 plans never carried `test_case_id`.

The migration test excludes `_sqlx_migrations` from its row-count comparison (that
ledger is *supposed* to grow) and reads the applied count from the ledger rather than
hardcoding it.

### 3.4 Minor observation

`0029` uses bare `CREATE INDEX idx_runs_release` while the other five use
`IF NOT EXISTS`. Immaterial under `sqlx::migrate!` (applied migrations are tracked by
checksum), but it is the one inconsistency in the set. Not worth changing now.

---

## 4. Project / tenant isolation

### 4.1 Every new store query

| Function | Statements | Scoped |
|---|---|---|
| `create_release` | INSERT (project_id column), SELECT by id | Yes |
| `list_releases` | `WHERE project_id = ?` | Yes |
| `runs_for_release` | `WHERE release_id = ?` | **No — see §4.3** |
| `upsert_module_node` | delegates to `upsert_quality_node` | Yes |
| `link_test_case_to_module` | delegates to `upsert_quality_node` + `upsert_quality_relation` | Yes |
| `backfill_task_test_case_links` | `WHERE project_id = ?`; UPDATE keyed on `run_id` | Yes |
| `insert_run_for_release` | `WHERE id = ? AND project_id = ?` — refuses otherwise | Yes |
| `table_ddl` / `table_exists` | `sqlite_master` (schema introspection, no tenant data) | N/A |

### 4.2 Verified on the migrated live database

```text
cross-project leaks:
  case id in another project's telemetry       0
  release in another project's runs            0
  node from another project's relation        0

orphan checks:
  telemetry with a test_case_id but no case   0
  tasks pointing at a missing case            0
  runs pointing at a missing release          0
  clusters pointing at a missing telemetry fp 0
  quality_nodes pointing at a missing project 0
  relations pointing at a missing node        0

timestamp format violations (non-RFC3339)    0
```

Plus, from the test suite: cross-project release attachment is **refused** *and leaves
no run behind*; 9 read surfaces asked about the other project leak none of 4
distinctive values; planning signals never cross a boundary.

### 4.3 One latent hazard — reported, not an active leak

`Store::runs_for_release(release_id)` takes **only a release id** and does not scope by
project. Every one of its six production call sites is guarded:

- `qa_memory.rs:622` — `rel` comes from `list_releases(project_id)`
- `qa_memory.rs:723` — validated against `list_releases(project_id)`, returns
  `UnknownSubject` first
- `qa_memory.rs:747`, `1350` — `other`/`rel` from the scoped list
- `qa_memory_risk.rs:415` — same `UnknownSubject` guard
- `qa_memory_risk.rs:483` — `other` from scoped `releases.iter()`

So **no leak today**. But the method is `pub`, so a future caller who passes a release
id without first scoping through `list_releases` would bypass the guard.

**Recommendation, not a defect:** change the signature to
`runs_for_release(project_id, release_id)` so the scope is enforced by the type rather
than by convention. Out of scope for this plan; would touch six call sites and two
tests.

---

## 5. Historical-data relationships

### 5.1 New relationships introduced

| Relationship | Storage | Verified |
|---|---|---|
| execution → test case | `tasks.test_case_id`, `execution_telemetry.test_case_id` | 0 orphans |
| run → release | `runs.release_id` (nullable, never auto-filled) | 0 orphans |
| cluster → telemetry | `failure_clusters.failure_fingerprint` | 0 orphans |
| case → module | `quality_relations(covers)` on the existing graph | 0 dangling |
| requirement → case | existing `quality_relations(validates)` | 0 dangling |

### 5.2 A pre-existing data condition, unchanged by this work

The live database contains **17 orphaned telemetry rows** of 19 — their `run_id` names a
run that was never persisted (the 2 persisted runs are the crawl and the handoff; the
other 17 telemetry rows reference run ids that never reached the `runs` table).

```text
LIVE      (27 migrations, untouched):  runs 2 · telemetry 19 · orphans 17
MIGRATED  (30 migrations):             runs 2 · telemetry 19 · orphans 17
VERDICT:  PRE-EXISTING, unchanged by these migrations
```

Migrations 0028–0030 only add columns and indexes; they never insert or delete rows, so
they could not have created or repaired this. It is reported as an observation about the
live data, not a defect of this plan. Note that `FINAL_VALIDATION_REPORT.md` §10 asserts
"no orphaned telemetry" — that assertion holds for fixtures built through production
code paths (where it is tested), and this live-data condition is a separate matter.

---

## 6. Defects found **during this review**

Two cosmetic, zero behaviour change, both fixed:

| # | Defect | Cause | Fix |
|---|---|---|---|
| R1 | `src/lib.rs:65` contained **ASCII 7 (BEL control character)** where `a` belonged — `daptive_intelligence` | a PowerShell double-quoted string where `` `a `` expanded to BEL during an earlier scripted patch | replaced the control character with `a`; full control-character sweep across all 9 new files now finds **none** |
| R2 | `src/store.rs` `create_release` held a dead `let _ = row;` and an unused binding | leftover from an earlier error-handling refactor | removed both; the INSERT result is genuinely unneeded |

A third candidate was investigated and found **not** to be a defect: the comment in
`adaptive_intelligence.rs` reading *"A read failure must not fail planning"*. Verified
against `qa_memory.rs:1258/1263/1348`, which use `unwrap_or_default()`, so a store read
failure genuinely yields an empty vector rather than an error. The comment is accurate
and was left alone.

Both fixes re-verified: build exit 0, clippy 0 errors, rustfmt clean, 1809 tests pass.

---

## 7. Secrets — nothing newly persisted

### 7.1 Literal credential scan

Pattern-matched `sk-*`, `ghp_*`, `AKIA*`, `aiqa_<40 hex>`, JWTs, PEM headers and
`password: "…"` across all 13 new files and all modified source files:

```text
literal credential patterns:  none
```

No `.sqlite`, `.env`, `.db`, `*.key` or any key file appears in the untracked set.
`.env` and `infra/data/*.sqlite3` are confirmed gitignored (`.gitignore:32`, `:37`).
The `infra/data/evidence/*.log` files found by a broader sweep are **pre-existing** and
already gitignored (`.gitignore:39`); they are not part of this change.

### 7.2 The word "secret" in the diff

Every occurrence is a **test fixture label**, and it is the correct label: tests name a
module `"secret"` precisely to prove it does *not* leak to another project
(`no_cross_project_leakage_anywhere_in_the_chain`). No credential material.

### 7.3 Does the QA memory layer persist anything new?

No. It **reads**. Every new table column is either a UUID, a nullable label, a timestamp,
or an index key:

```text
tasks.test_case_id                   TEXT    (a UUID, or NULL)
runs.release_id                     TEXT    (a UUID, or NULL)
failure_clusters.failure_fingerprint TEXT   (a SHA-256 hex, or NULL)
qa_releases                          label/version/notes/created_by/created_at
```

No new column stores a credential, a token, or any secret-bearing value. The retrieval
layer never reads `params` or echoes `error_message` — verified by
`no_credential_material_reaches_the_memory`, which seeds a secret into both and
serialises all six surfaces.

---

## 8. Pre-existing finding — `runs.plan` → `TaskDef.params` (reported separately)

**Not modified. Confirmed not required for QA Intelligence Memory correctness.**

### 8.1 The finding

`runs.plan` stores `TaskDef.params` verbatim, including credentials. A task given
`{"username": "qa", "password": "<secret>"}` has that secret persisted as given.
`adaptive_intelligence::mask_dynamic_tokens` scrubs telemetry text (UUIDs, timestamps,
IPs, ports, hex, digits) but is never applied to the run plan.

### 8.2 Why QA memory does not need it fixed

| Check | Result |
|---|---|
| Does `qa_memory.rs` reference `plan` or `params`? | **No** — zero references (all grep hits are the phrase "master plan" in comments) |
| Does `qa_memory_risk.rs`? | **No** — zero |
| Does the new backfill read the plan? | Yes, but reads **only** `tasks[].test_case_id` and writes **only** that field on `tasks`. It never copies `params` anywhere and never logs the plan. |

So retrieval correctness does not depend on the plan's contents.

### 8.3 Why it was left alone

It is run-plan persistence, not QA memory. Changing it means deciding what the run engine
persists for every executor. The relevant unknown: `store.rs:1290` reads
`SELECT plan FROM runs WHERE id = ?` in production code, and `rbac_probe` and the audit
log also read plans — so a redaction change needs checking against those readers, not
guessing. Master plan §14 forbids rewriting existing architecture unnecessarily, and the
review instruction was explicit: do not modify unless required for correctness.

### 8.4 Recommendation

Persist a redacted copy of `params` in `runs.plan` while leaving the in-memory `TaskDef`
untouched for execution — but only after verifying that `store.rs:1290`, `rbac_probe` and
the audit-log readers do not depend on `params` being present. This is a separate change
with its own review.

---

## 9. Stray / generated / temp / debug files

```text
.log / .tmp / .bak / .orig / .rej / .swp / .pyc / .exe / .dll / core / .DS_Store
  → none in the change set
```

| Item | Verdict |
|---|---|
| scratch / probe / debug files | none — the one `probe_graph_state` test I wrote during Phase 6 debugging was removed before the suite went green |
| temp paths, user paths, API keys inside the reports | **none** — scanned `docs/qa-intelligence-memory/*.md` for `AppData`, `Temp\opencode`, `C:\Users`, `aiqa_<hex>` |
| sqlite / `.env` / `*.key` in the untracked set | none |
| intended-vs-actual manifest comparison | **exact match** — 12 intended new `services/` files, 12 actual, nothing extra, nothing missing |
| dangling report cross-references | **none** — every `PHASE_*` file referenced by any report exists |
| false claims in the reports | none — `FINAL_VALIDATION_REPORT.md` does **not** claim Phase 3/4/5 report files exist; it tabulates the phases instead |

---

## 10. Test results

| Check | Command | Result |
|---|---|---|
| Backend suite | `cargo test` | **1809 passed / 0 failed / 2 ignored**, exit 0 |
| Clippy | `cargo clippy --all-targets` | **0 errors** |
| Clippy, changed files | same | 3 warnings, **all pre-existing** (verified against `HEAD`: `store.rs:59`, `:85` doc-comment spacing, `:4045` an 8-arg function that predates this work) |
| Rustfmt | `cargo fmt --check` | **0 diffs in any file this plan created or edited** (125 repo-wide diffs, all pre-existing) |
| Control characters | char sweep over 9 new files | **none** |
| Migration on a live copy | `cargo test --lib qa_memory_migration` | **1 passed** — 30 migrations applied, 19 telemetry rows preserved, 0 pre-linked |
| TypeScript | `npx tsc --noEmit` | **clean**, exit 0 |
| Frontend | `npm test` | **1605 passed / 79 files**, 0 failed |

One note on method: my first frontend check reported "FAILURES" from a `Select-String
'FAIL'` match. That was a **false positive** — it matched the word inside test *names*
(`"...a crawl that genuinely fails..."`). Verified against real failure markers
(`✗`, `FAIL `, `failed (`): zero. Reporting it because the first reading was wrong and
silently accepting it would have been the wrong move.

Integration/E2E coverage within the backend suite: `api_tests.rs` (741 KB, the HTTP
layer), `key_scope_tests.rs`, `rbac_matrix_tests.rs`, `rbac_probe_tests.rs` (a real
loopback probe, not a declared matrix), `operations_tests.rs`, `store_tests.rs`,
`governed_healing_tests.rs`, `agent_execution_tests.rs`, plus the 93 tests added by this
plan. The frontend suite's 79 files include the live navigation and crawl-grounding
integration tests.

---

## 11. Blocking issues

| # | Issue | Severity | Status |
|---|---|---|---|
| B1 | `PHASE_3_*`, `PHASE_4_*`, `PHASE_5_*` and `PHASE_1_REPORT.md` were never written, though the plan's rules #7/#8 require them and those phases are otherwise complete and tested | Process / deliverable | **CLEARED 2026-10-08** — seven documents written, no code or test touched |

Nothing else blocks. The code is safe, additive, tested and tenant-isolated.

Clearing B1 was done by **writing the documents**, which is what the finding asked for —
not by modifying application logic, and not by softening any test or claim. See the B1
resolution record in §0.

---

## 12. Recommended commit message

```
QA Intelligence Memory: durable history, retrieval, planning risk, validation

Implements the six-phase QA Intelligence Memory plan. Phase 1 found that
execution_telemetry.test_case_id was NULL in 19 of 19 rows and that release,
module and environment had no identity at all, so this extends the existing
backbone rather than adding a second one.

Phase 2 - memory foundation (migrations 0028, 0029)
  tasks.test_case_id makes the case<->execution link durable; it was
  previously recoverable only from the runs.plan JSON. qa_releases adds the
  release identity Release Risk needs. Modules become quality_nodes vertices
  with a `covers` edge, not a table. backfill_task_test_case_links recovers
  only unambiguous historical links and refuses to guess.

Phase 3 - retrieval (qa_memory.rs)
  Deterministic, project-scoped module risk, test stability, release,
  requirement and bug history. Empty history returns INSUFFICIENT_DATA rather
  than zeroes.

Phase 4 - planning intelligence
  QaIntelligenceFeedback gains structured PlanningSignal values carrying a
  metric, evidence, confidence and a 90-day window, so a plan can be traced
  to what produced it. No evidence means no signal.

Phase 5 - risk (qa_memory_risk.rs)
  Module and release risk on the existing calibrate_risk, not a second score.
  Three of its six factors have nowhere to come from in this schema, so model
  coverage is reported numerically instead of a score being invented.

Phase 6 - validation
  The whole chain runs end to end, security and integrity are asserted, and
  performance is measured rather than assumed: every retrieval query is a
  SEARCH on a named index, 480 telemetry rows read in 33ms worst case,
  319 bytes stored per remembered execution.

Three real defects found and fixed, all reporting absence where data existed:
  - bug_history passed a cluster id where a signature was expected, so every
    project reported no recurring defects
  - it then compared a human-text signature against a SHA-256 fingerprint,
    so every defect reported touching no tests (migration 0030 joins them)
  - release_risk counted the target release twice in its own trend

All five migrations are additive. Verified against a copy of the live database:
every table's row count unchanged, no historical row given an invented link.
No existing test weakened, skipped or disabled.

Backend 1809 passed / 0 failed. Frontend 1605 passed. tsc clean. clippy 0 errors.
rustfmt clean for every file touched.
```

### If you prefer separate commits

1. `Migrations 0028-0029 + durable test-case link, releases, modules` — phase 2
2. `Retrieval layer: module risk, stability, release/requirement/bug history` — phase 3
3. `Planning signals with metric, evidence, confidence and window` — phase 4
4. `Module and release risk on the existing calibration` — phase 5
5. `Migration 0030 + end-to-end validation, security, integrity, measurement` — phase 6
6. `docs: phase reports and final validation report`

Phase 6's migration depends on the retrieval layer, so it cannot be split before
phases 2–3 land. Note that only commit 1 and 5 change schema, so those are the ones
worth reviewing with extra care.