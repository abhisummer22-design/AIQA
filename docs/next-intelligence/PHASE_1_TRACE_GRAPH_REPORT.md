# Phase 1 — Trace Graph Completion

**Phase 1 of `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`. Backend only. PASS.**

---

## 1. Root cause: why `quality_relations` held 0 rows

The plan asked for the cause to be determined, not guessed. It was investigated by
reading every writer, every reader, and every call site, and then confirmed against the
live database. **It was not one cause and not a data problem.** Three independent code
defects had to be true at once for the table to stay empty, and each is verified below.

### 1.1 Two `node_type` values for one concept — the decisive defect

| Path | Wrote | `external_ref` | Read by |
|---|---|---|---|
| `Intelligence::link_test_case_to_requirement` | `"test_case"` | `test_cases.id` ✅ | — |
| `Store::list_test_cases_for_requirement` (SQL) | — | — | `"test_case"` |
| `AdaptiveIntelligenceService::record_execution_telemetry` | `"test"` | `telemetry.task_name` ❌ | — |
| `AgentExecutionDispatcher` | `"test_case"` | `task.name` ❌ | — |
| `qa_memory.rs` (`test_stability` and 3 others) | — | — | `"test"` |

`quality_nodes` has identity `(project_id, node_type, external_ref)`
(`Store::upsert_quality_node`). A case's **requirement** edge and its **execution's
failure** edge therefore landed on two vertices with different `node_type`s, which can
never satisfy that identity. Both edges could exist and still describe two unrelated
things.

This was not a benign inconsistency. `qa_memory.rs:527` already carried the comment
*"The case vertices carry `external_ref` = the case id used on `covers`"* — the
intended design was already `"test"` keyed on the case id. The writer disagreed with the
documented intent and with every reader.

### 1.2 A name where an identity belonged

Three writers keyed `external_ref` on `task_name`:

* `record_execution_telemetry` → the `fails_with` subject vertex
* `AgentExecutionDispatcher` → the `validates` subject vertex
* `Intelligence::update_test_intelligence` → the vertex anchoring `risk_assessments`
  and `test_value_assessments`
* `governed_healing` → the `healed_by` / `regressed_by` subject vertex

A name collapses under rename and merges two distinct cases that share a title. Worse,
a name sits in the same key space as a UUID, so a task literally named after a
`test_cases.id` would silently claim that case's vertex and attach its risk score to it.

### 1.3 `covers` had no production caller

`Store::link_test_case_to_module` and `Store::upsert_module_node` existed and were
exercised **only by their own tests**. A non-test reference sweep found **1 occurrence
of the function name in non-test code — the declaration itself.** The `covers` relation
type was unreachable by construction.

### 1.4 What was *not* the cause

Recording this explicitly, because each was checked and ruled out rather than assumed:

* **Not a migration problem.** `quality_relations` has been real schema since Phase 1 of
  the earlier plan. No DDL was missing.
* **Not a transaction problem.** No rollback path touches these writes.
* **Not "the code path was never reached".** The `validates` writer *was* reached —
  `api.rs::create_test_case_scoped` calls it. It returned early, correctly, because
  **both** live `test_cases` rows have `requirement_reference = NULL` **and**
  `source_requirement = NULL`, and the writer early-returns when there is no real
  requirement identity. That is right behaviour (no requirement text, no requirement
  vertex), not a bug — and it is the only part of the emptiness that was data, not code.
* **Not a foreign-key or index problem.** Confirmed no constraint dropped the rows.

### 1.5 Live-data confirmation

Read from `infra/data/orchestrator.sqlite3` (still at 27 migrations — this work never
migrated it in place):

```
quality_nodes        2      both node_type='test', external_ref = a task name
quality_relations    0
execution_telemetry  19     0 with test_case_id; 6 with category+fingerprint
failure_clusters     0      bug_drafts 0, requirements 0, qa_releases absent
test_cases           2      requirement_reference NULL, source_requirement NULL
```

The two `quality_nodes` rows are the residue of §1.2: `"test"` vertices keyed on a task
name, which is why `qa_memory` found subjects but no edges.

---

## 2. Files changed

10 modified, 2 added. No file outside this list was touched.

| File | Change |
|---|---|
| `store.rs` | `TEST_NODE_TYPE` / `LEGACY_TEST_NODE_TYPE` / `TASK_NODE_TYPE`; `unambiguous_task_ref`; `upsert_test_case_node`; `upsert_task_node`; `link_case_to_requirement`; `link_case_to_module`; `get_failure_cluster_by_fingerprint`; `get_task_test_case_id`; `list_test_cases_for_requirement` reader; `link_test_case_to_module` uses the constant |
| `trace.rs` | **new** — `TraceReader` (`index`, `walk_test_case`) + types |
| `trace_tests.rs` | **new** — 21 tests |
| `intelligence.rs` | `link_test_case_to_requirement` → canonical writer; `update_test_intelligence` takes `test_case_id`; risk/value vertex keyed on the case |
| `adaptive_intelligence.rs` | `fails_with` subject vertex resolved from `test_case_id`; **panic fix** (§7) |
| `agent_execution.rs` | `validates` subject vertex → `TASK_NODE_TYPE` |
| `governed_healing.rs` | `healed_by` / `regressed_by` subject vertex resolved via `tasks.test_case_id` |
| `api.rs` | one route + `get_trace_scoped` |
| `api_tests.rs` | one HTTP end-to-end test |
| `agent_execution_tests.rs` | `test_28` rewritten (it asserted against a vertex it created itself — §7) |
| `lib.rs` | `pub mod trace` + `mod trace_tests` |
| `qa_memory.rs` | `TELEMETRY_READ_MAX` made `pub` (the reader reuses the existing bound rather than inventing one) |

### Migrations changed: **none**

Phase 1 is additive in behaviour only. No DDL was added, so there was no migration to
make safe. This is asserted as a fact rather than claimed:
`trace_tests::phase_one_introduces_no_new_migration` pins the applied migration count at
30 and fails if it moves. The test also asserts each column the walk depends on already
exists, each against the table that actually carries it.

---

## 3. Relations added and fixed

| Relation | Before | After |
|---|---|---|
| `validates` (case → requirement) | written to `"test_case"`; readable only by the reader that also used `"test_case"`; `agent_execution` keyed it on a name | canonical `"test"` vertex keyed on `test_cases.id`; agent tasks use `TASK_NODE_TYPE` |
| `fails_with` (case → cluster) | `"test"` vertex keyed on `task_name` | keyed on `test_case_id` where present, else `TASK_NODE_TYPE` |
| `covers` (case → module) | no production caller | still **no production caller, deliberately** — see §8 |
| `healed_by` / `regressed_by` | `"test"` vertex keyed on `proposal.task_name` | resolved through `tasks.test_case_id`, else name via `unambiguous_task_ref` |

**No new relation type was invented.** `derived_from`, `retests` and `fixed_by` were
*not* added: the domain model has no `fixed_by` source (healing verification is recorded
on the proposal row, not as an edge), and manufacturing either would be a fabricated
link. The walk reports `fix` and `retest` from the evidence that does exist —
`healing_proposals.failure_fingerprint` and its `verification_result`.

### IDs used

* **Case identity:** `test_cases.id` (UUID) — the key for `quality_nodes.external_ref`
* **Case ↔ execution:** `execution_telemetry.test_case_id` / `tasks.test_case_id`
* **Task ↔ case resolution:** `tasks.test_case_id`, project-scoped via a join to `runs`
* **Failure:** `execution_telemetry.failure_fingerprint` → `failure_clusters.id`
  (the graph vertex is keyed on the fingerprint; the row is keyed on `signature`; the
  `failure_fingerprint` column added by migration 0030 is the join — new lookup
  `get_failure_cluster_by_fingerprint`)
* **Release:** `runs.release_id` → `qa_releases.id`
* **Fix / retest:** `healing_proposals.id`, joined on `failure_fingerprint`
* **Task with no case:** `task_name`, in a separate `node_type`

`task_name` survives as an identity in exactly one place — the `TASK_NODE_TYPE`
namespace, where readers do not join on it — and is additionally routed through
`unambiguous_task_ref` so a UUID-shaped name cannot escape into the case namespace.

---

## 4. APIs added / changed

**One** route:

```
GET /api/v1/projects/:project_id/trace[?test_case_id=<uuid>]
```

* no `test_case_id` → the project's trace **index**: each case vertex, whether it
  resolves to a real `test_cases` row, which hops exist, which are missing
* with `test_case_id` → the full walk:
  `Requirement → Test → Execution → Failure → Bug → Fix → Retest → Release`

Guarded by `require_scoped_project`, then `project_id` reaches every query in the reader.
A malformed id is `400 BAD_REQUEST`, not an empty trace.

No existing route changed shape. No frontend caller was added — see §9.

---

## 5. Trace retrieval behaviour

Every hop carries the evidence it came from (`TraceEvidence { via_node, via_relation,
rows, missing }`), so a claim can be checked rather than trusted. A hop with no evidence
is **absent** from `steps` and **named** in `incomplete`.

---

## 6. Tests added

**21** in `trace_tests.rs` + **1** HTTP end-to-end in `api_tests.rs`.

| Area | Tests |
|---|---|
| Relation persistence | `validates` persists **and** is readable; legacy node type still readable; no requirement ⇒ no edge |
| `covers` | persists; `validates` and `covers` share one vertex |
| `fails_with` | uses the case id; uses the task namespace without one; a pass writes none |
| Identity guard | ordinary name passes through; UUID-shaped name is namespaced; the 3 node types stay distinct |
| Short/multibyte fingerprint | ingest must not panic (§7) |
| Idempotence | 3× the same two edges ⇒ 1 row each; reader returns 1 case |
| Project isolation | same requirement key in 2 projects; reader, index, and crossed walk all stay in-project |
| Walk honesty | full chain returned; index lists real cases and their gaps; empty project ⇒ empty; no history ⇒ all 8 hops named missing; fingerprint with no cluster row reported as incomplete |
| Task ↔ case | resolves within project; refuses across projects; a case-less task stays `None` |
| Migration | count pinned at 30; 7 required columns asserted present |
| HTTP e2e | `POST /test-cases` ⇒ relation row in SQLite ⇒ `GET /trace` returns that chain; `incomplete` populated and `steps` absent for unevidenced hops; malformed id ⇒ 400; other project resolves nothing |

The negative tests (`..._does_not_...`, the `missing` assertions) are the point: a suite
that only tests happy paths cannot detect fabricated traceability.

---

## 7. Bugs found and fixed

### 7.1 Panic in the failure-graph writer — pre-existing, found by a Phase 1 test

`adaptive_intelligence.rs` built the cluster title with `&fingerprint[..12]` — a **byte**
slice on a `&String`. Any fingerprint under 12 bytes panicked the whole telemetry
ingest, losing the execution record and every edge it would have produced. A 12-character
multi-byte fingerprint would also panic, by splitting a `char` boundary.

It had never fired because every fingerprint the code produces is a 64-char SHA-256 hex
digest, so the short case looked unreachable. It was reachable via any other writer of
that column.

**Fixed** with `fingerprint.chars().take(12)`. **Guarded** by
`a_short_or_multibyte_fingerprint_does_not_panic_the_ingest`, which asserts 2-byte,
10-byte, 21-byte and 24-byte fingerprints all ingest *and* still produce their edges.
The guard is proven, not assumed: it reproduced the exact panic
(`byte index 12 is out of bounds for string of length 10`) before the fix.

### 7.2 Cross-tenant leak in the new walk — introduced by Phase 1, caught by its own test

`TraceReader` read the case row through `Store::get_test_case(id)`, which takes a bare id
with **no tenant argument**. A caller holding another tenant's case id got that tenant's
`requirement_reference`, `source_requirement` and `executed_run_id` back in the `test`
hop.

Not hypothetical — `a_project_cannot_trace_another_projects_case` returned another
tenant's requirement hop on its first run. **Fixed** with `TraceReader::case_in_project`,
which re-checks `project_id` rather than trusting the caller. A case the walk cannot see
is now reported absent rather than as another tenant's data.

### 7.3 A test that asserted against a vertex it created itself — pre-existing

`agent_execution_tests::test_28` called `upsert_quality_node(pid, "test_case", "Verify
Login Flow", ...)` itself, then asserted the relation on the resulting id. Because the
unique key is `(project_id, node_type, external_ref)`, the id matched the dispatcher's
only while both used the same literal `node_type`. The coupling was real but expressed as
a duplicated literal; when the dispatcher changed to the honest identity, the test's
`upsert` silently invented a *second* vertex and failed on a node it had just created.

**Rewritten** to observe the dispatcher's vertex (using the constant production uses),
assert the `validates` edge runs from it, and assert **negatively** that no case vertex
was fabricated from the task name. Strictly stronger, and it now fails if the dispatcher
stops writing.

### 7.4 Wrong test premise, corrected rather than worked around

`phase_one_introduces_no_new_migration` initially asserted `runs.release_id` on
`execution_telemetry`. It does not exist there. The test failed rather than passing
silently, and was corrected to assert each column against the table that actually carries
it — a case where a failing test was the right outcome.

---

## 8. `covers` remains unwritten in production — deliberately

The plan asks that `covers` "persist in real execution flows". **It does not, and it
cannot without a decision this phase is not authorised to make.**

`link_case_to_module` is now correct (canonical vertex, stable id, tested, wired into the
walk) and idempotent. But nothing in the production data model names a module:

* `TestCaseRow` has `source_document` — a document, not a module
* `TaskDef` has no module field (verified against the struct, not assumed)
* `RunPlan` has no path
* the only `source_file` in the codebase is `CreateCustomSkillBody.source_file`, for
  importing a `SKILL.md` — unrelated

Adding a caller would mean inventing a document→module or name→path mapping: a `covers`
edge that looks like traceability and is a guess. That is the precise failure mode this
phase exists to remove, so the walk reports `module` in `incomplete` instead.

Closing it needs a real module identity — most plausibly a column on `test_cases` — which
is a schema decision for the user, not a Phase 1 implementation detail. **Recorded as
limitation L1 (§9).**

---

## 9. Remaining limitations

**L1 — `covers` is never written.** No module identity exists in the domain model. Needs
a schema decision. (§8)

**L2 — risk/value history is stranded by the identity change.** For a case-backed task,
`upsert_risk_assessment` and `upsert_test_value` now anchor to the case vertex instead
of the old name-keyed vertex. Rows written before this change stay on the old vertex and
are no longer read through the new id. Nothing is deleted and no score is wrong; the
history is simply not carried across. A backfill would need the old name→case mapping,
which does not exist.

**L3 — `update_test_intelligence` still scores history by task name.**
`task_history_by_name(&t.name, Some(project_id), 20)` matches on name. It is
project-scoped (the B3 fix), so no cross-tenant leak, but two same-named tasks inside one
tenant still share a failure rate. Moving it to id-based history changes risk *scores*,
so it belongs to the risk phase, not here.

**L4 — legacy `"test_case"` edges remain as a separate vertex class.** They are read, so
no history is lost, but a case with only legacy edges has two vertices and the index
reports both. The alternative — a data migration rewriting them — is destructive and was
excluded.

**L5 — no frontend surface.** The endpoint is backend-only; no UI or TypeScript was
added or changed. Frontend tests are therefore unaffected: 1605 tests / 79 files at
baseline, untouched. A trace panel is Phase 1 follow-on work, not a gate for it.

**L6 — no E2E CI job.** Pre-existing gap: `ci.yml` has 3 jobs and none is E2E or
integration. The new HTTP test runs in `cargo test`, not in a separate pipeline.

**L7 — the live database is unchanged and still empty.** This work never migrated it in
place; it remains at 27 of 30 migrations with `quality_relations = 0`. Its two
`test_cases` have no requirement reference and its 19 telemetry rows have no
`test_case_id`, so **even after migrating, the old rows do not become a trace**. Only new
activity writes edges: a case created with a requirement reference, or a failure whose
telemetry row carries a case id. Fixing existing rows would need the backfill in L2, plus
a way to attribute the 17 orphaned telemetry rows.

---

## 10. Test results

| Run | Before | After |
|---|---|---|
| Backend suite | 1809 passed / 0 failed / 2 ignored | **1831 passed / 0 failed / 2 ignored** (+22) |
| New tests | — | 21 (`trace_tests`) + 1 (`api_tests` HTTP e2e) |
| `cargo build` | clean | clean |
| `cargo clippy --all-targets` | 0 errors | 0 errors; no warning in `trace.rs` / `trace_tests.rs` |
| `rustfmt --check` on the 9 files this phase changed that were fmt-clean at HEAD | — | clean |
| Frontend | 1605 / 79 files | unchanged — no frontend file touched |

Zero regressions: no pre-existing test was weakened, skipped or deleted.

Two formatting notes, stated precisely because a blanket "fmt clean" would be false:

* **`api.rs`, `api_tests.rs` and `lib.rs` fail `rustfmt --check` — at HEAD too.**
  Verified by extracting each from `git show HEAD:…` to a byte-exact temp file and
  running `rustfmt --check`: all three already exited 1 before this phase. `lib.rs`
  because rustfmt reorders `mod` declarations and the file is not in alphabetical order;
  `api.rs` / `api_tests.rs` because of pre-existing comment and string drift. This phase
  introduced none of it. CI does not gate on it either — `.github/workflows/ci.yml` runs
  `cargo fmt --all` in **write** mode, never `--check`.
* **`cargo fmt --all` was run, and its reformatting of 16 files unrelated to this phase
  was reverted** so the diff stayed reviewable. Those 16 files are byte-identical to HEAD
  again. The 12 files this phase does change are listed in §2.

---

## 11. Phase 0 audit correction

`docs/next-intelligence/PHASE_0_FEATURE_AUDIT.md` §1 count table corrected:
`EXISTS 4 / PARTIAL 5 / MISSING 4 / DUPLICATE_RISK 1` → **`EXISTS 2 / PARTIAL 5 /
MISSING 7 = 14`**, with `DUPLICATE_RISK 1` stated as an **orthogonal flag** on one of
those 14, excluded from the partition.

The correction went **against the numbers in the instruction**, deliberately. The
instruction specified `EXISTS 4 / MISSING 5`; the audit's own per-feature sections
(§3.1–§3.14) classify `EXISTS` = §3.1, §3.11 and `MISSING` = §3.4–§3.9, §3.14 — 2 and
7. Writing 4/5/5 would have made the table disagree with the sections it summarises and
left `4 + 5 + 5 ≠ 14`. The sections were treated as authoritative; the probable origin of
the wrong number is set out in the correction note (§4 of that audit lists 16 existing
capabilities, a different set from the 14 plan features).

**No application logic was changed for this correction.**

---

## 12. PASS gate

| Criterion | Status | Evidence |
|---|---|---|
| Actual relation rows are persisted | **PASS** | `requirement_edge_persists_to_the_canonical_node_and_is_readable`, `module_edge_persists_...`, `failure_edge_uses_the_stable_case_id...`, HTTP e2e asserting the row in SQLite |
| Stable IDs are used | **PASS** | `test_cases.id`, `execution_telemetry.test_case_id`, `tasks.test_case_id`, `failure_clusters.id`, `runs.release_id`, `healing_proposals.id`. `task_name` confined to `TASK_NODE_TYPE` + `unambiguous_task_ref` |
| Trace retrieval works | **PASS** | `the_walk_returns_the_chain_that_is_actually_persisted`; HTTP e2e |
| No fabricated links | **PASS** | 8 negative tests: pass ⇒ no `fails_with`; no requirement ⇒ no `validates`; no case ⇒ `TASK_NODE_TYPE` not a case vertex; fingerprint w/o cluster ⇒ reported incomplete; no history ⇒ 8 hops named missing; empty project ⇒ empty index; crossed walk ⇒ nothing; no case vertex from a task name |
| Project isolation passes | **PASS** | `a_project_cannot_trace_another_projects_case`, `a_task_resolves_its_case_id_and_respects_the_project`, HTTP e2e, plus §7.2 |
| All new tests pass | **PASS** | 22/22 |
| Relevant existing regression passes | **PASS** | 1831 passed / 0 failed; 0 weakened |
| No duplicate subsystem introduced | **PASS** | no new table, no new store, no new engine. One reader over the existing graph; 1 new store lookup; 1 new store getter |
| Migration safety | **PASS (N/A)** | no migration created; count pinned by test |

### Honest caveats on this gate

Two criteria pass in a narrower sense than the wording suggests, and are recorded rather
than glossed:

1. **"Actual relation rows are persisted"** is proven in a **fresh migrated database
   driven through real production writers**, not in the live database. The live database
   is untouched and empty, and would remain empty for its existing rows even after
   migrating (L7). This is the plan's required distinction between *capability exists*,
   *exercised by real data*, and *fixture-only* — and only the first is claimed.
2. **`covers` does not "persist in real execution flows"** (§8, L1). The gate item is
   met for `validates` and `fails_with`; for `covers` the correct answer today is that
   the edge cannot be written without an invented mapping, and the walk says so.

**Phase 1 = PASS**, with L1 and L7 carried forward as known gaps rather than closed by
assertion. Phase 2 not started. Nothing committed or pushed.