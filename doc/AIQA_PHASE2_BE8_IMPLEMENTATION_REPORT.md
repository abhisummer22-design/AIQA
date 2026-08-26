# AIQA — Phase 2 — BE-8 Implementation Report

**Document:** `AIQA_PHASE2_BE8_IMPLEMENTATION_REPORT.md`
**Date:** 2026-08-17
**Unit:** BE-8 — API / Project Scoping (Projects API, `/api/v1` foundation)
**Scope:** Backend only. Zero frontend files opened or modified.

---

## 1. Source-of-Truth Reading and Scope Resolution

Read in full before writing code: `AIQA_API_CONTRACT_v1.0.md`, `AIQA_ACCEPTANCE_MATRIX_v1.0.md`, `AIQA_INTEGRATION_IMPLEMENTATION_PLAN_v1.0.md`, `AIQA_BACKEND_IMPLEMENTATION_PLAN_v1.0.md`, `AIQA_PHASE1_FINAL_INDEPENDENT_VERIFICATION_REPORT.md`, and the current backend source (`api.rs`, `auth.rs`, `store.rs`, `domain.rs`, `state.rs`, `main.rs`, all three existing migrations) — not assumed identical to the documents.

**Two conflicts were found between documents and resolved per the instruction to follow the existing plan rather than invent behavior:**

1. **Scope of "BE-8" itself.** The Integration Implementation Plan's own milestone table (§15) defines `BE-8 = "Projects API"` and separately `BE-9 = "/api/v1 + scoped routing (all routes contract-shaped)"` as two distinct units. This session's instructions relabel `BE-8 = "API / Project Scoping"` and list `BE-9 = "Structured errors"`, which does not by itself account for moving the other 16 existing routes under `/api/v1`. Resolution: this unit implements the Projects resource itself (Contract §5.1–5.4) newly mounted under `/api/v1/projects`, demonstrating `/api/v1` routing and project/tenant isolation end-to-end for a real resource — but does **not** move the 16 pre-existing routes (`/runs`, `/bug-drafts`, etc.). Moving those remains later, separate work not started here. This is the narrower, safer reading, consistent with "do not jump ahead" and with not re-touching 16 already Phase-1-hardened endpoints in the same unit that introduces a brand-new entity.

2. **Migration scope for the `projects` table.** The Backend Implementation Plan §8 describes `0004_projects` as including "FK from `runs`; backfill existing `project_id` values." Resolution: **not done in this migration.** `runs.project_id` (and the same column on `api_keys`, `quality_nodes`, `failure_clusters`, `audit_log`) holds live, Phase-1-verified free-text values (`"alpha"`, `"beta"`, `"default"`, `p1`, `NULL`, etc.) that predate any project entity. Retrofitting a foreign key onto them requires inventing a `projects` row for every distinct existing string first — a real, correctness-sensitive backfill migration across tables outside BE-8's stated scope. The same Backend Plan document lists this as its **own**, separate migration (`0005_project_denormalisation`), so deferring it is consistent with that document's own structure, not a deviation from it. Per the task's Database Safety instruction, a schema-affecting change should be justified by what actually consumes it — nothing in BE-8 consumes a FK relationship yet, since the existing `require_project_access` isolation mechanism (already verified in Phase 1) compares plain strings and is completely independent of whether a `projects` row exists for that string.

**Contract §5.5 (`GET .../projects/{id}/context`)** was read and intentionally **not implemented** in this unit: its payload is context/ingestion data (PRD/FRD/SRD/API-spec/etc. references), which the Integration Plan's own gap matrix marks `MISSING` and schedules under Phase 5 ("context ingestion" — item 29). Implementing it now would mean inventing an ingestion data model out of order. Sections 5.1–5.4 (the actual Project entity: List/Create/Get/Update) are complete.

---

## 2. Step 1 — Gap Matrix (Source-Driven, Before Any Code)

| Requirement | Contract | Current Backend (before this unit) | Gap | Required Change | Test |
|---|---|---|---|---|---|
| Project entity | §4, §5 | No `projects` table anywhere in migrations 0001–0003; `project_id` is a free-text attribute on `api_keys`/`runs`/`quality_nodes`/`failure_clusters`/`audit_log`, never validated against a canonical list | MISSING | New migration: `projects` table | store-level |
| List Projects | §5.1 `GET /api/v1/projects` | No route | MISSING | New handler + route; isolation via `ctx.project_id` filter (same convention as `list_runs`/`list_bug_drafts`) | store + live HTTP |
| Create Project | §5.2 `POST /api/v1/projects` | No route | MISSING | New handler + route; Admin-only (matches `/admin/api-keys` precedent) | store + live HTTP |
| Get Project | §5.3 `GET /api/v1/projects/{id}` | No route | MISSING | New handler + route; fetch-then-`require_project_access` (matches `get_run`'s existing order) | store + live HTTP |
| Update Project | §5.4 `PATCH /api/v1/projects/{id}` | No route | MISSING | New handler + route; Operator + `require_project_access` (matches `create_run`/`approve_run` precedent); partial update via `COALESCE` | store + live HTTP |
| Project Context | §5.5 | No route; no ingestion data model | MISSING, deferred | Out of scope — depends on §6 Context Ingestion (Integration Plan Phase 5) | none this unit |
| `/api/v1` routing | §2.1 | 0 of 16 existing routes carry it | PARTIAL (systemic) | This unit adds `/api/v1/projects*` only; the other 16 routes are unmoved | live HTTP: new routes respond under the prefix, old routes unaffected |
| Existing resource isolation (runs/evidence/etc.) | §2.3 | Already enforced, Phase-1-verified | No gap | Not touched | regression only |
| Isolation on the new resource | §2.3 | N/A — resource didn't exist | MISSING | `require_project_access` applied to Get/Update; filter applied to List | live HTTP: cross-project 403, same-project 200, scoped list count |
| Invalid project behavior | (implicit — §2.5 error handling) | N/A | MISSING | 404 on unknown id (Get/Update), 400 on blank `name` (Create/Update) | live HTTP |
| Error response format | §2.5 structured envelope | Plain `(StatusCode, String)` everywhere | PARTIAL (systemic, pre-existing) | Out of BE-8 scope — this session's own "BE-9" unit | none this unit |
| Existing RBAC behavior | §26 | `Role::Viewer < Operator < Admin`, Phase-1-verified | No gap | Reused exactly (`require_role`), not modified | regression |
| Status vocabulary | §13 | `INFRA_ERROR` vs `INFRASTRUCTURE_ERROR` mismatch (pre-existing, documented in Phase 1 verification) | PARTIAL (systemic, pre-existing) | Not touched — no status vocabulary appears anywhere in the Project resource | none this unit |

---

## 3. Requirement / Existing Behavior / Gap / Root Cause

**Requirement:** API Contract §5.1–§5.4 — a Project resource with server-side isolation, under `/api/v1/projects`.

**Existing behavior (before this unit):** No project entity of any kind. `project_id` existed only as a nullable, unvalidated string attribute scattered across five tables, with isolation enforced purely by string equality in `auth::require_project_access` — real and already Phase-1-verified for *existing* resources (runs, evidence, bug drafts), but nothing to create, list, fetch, or update a project *as its own resource*.

**Root cause:** never built. This is new capability, not a regression or defect — Phase 1's entire scope was correctness/persistence of the execution engine; the Projects resource was explicitly out of scope until Phase 2 (Integration Plan §20 "Go condition").

---

## 4. Implementation

### 4.1 Migration

**File:** `services/orchestrator/migrations/0004_projects.sql` (new)

```sql
CREATE TABLE projects (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
```

No change to any existing table. Applied automatically and idempotently by the existing `sqlx::migrate!("./migrations").run(&pool)` call in `Store::connect` — the same mechanism that has applied migrations 0001–0003 since Phase 1, verified live against the real production database (§6).

### 4.2 Store layer

**File:** `services/orchestrator/src/store.rs`

- New `Project` DTO (row-DTO section, matching `EvidenceRow`/`ApiKeyListRow`'s existing style): `{id, name, description, created_at, updated_at}`.
- `insert_project(id, name, description) -> Result<()>` — id generated by the caller (`Uuid::new_v4()`), matching every other entity in this store.
- `list_projects(project_filter: Option<&str>) -> Result<Vec<Project>>` — `None` = every project; `Some(pid)` = at most the one project with that id. Same `Option<&str>` filter convention already established by `list_bug_drafts`/`list_test_intelligence`/`task_history_by_name`.
- `get_project(id) -> Result<Option<Project>>` — single lookup, used for `GET` and as the existence check before `PATCH`.
- `update_project(id, name: Option<&str>, description: Option<&str>) -> Result<bool>` — `COALESCE`-based partial update (same pattern `set_run_finished` already uses for `policy_notes`); returns whether a row was actually affected, so the caller can 404 on an update to a nonexistent project instead of reporting a false "ok".

### 4.3 API layer

**File:** `services/orchestrator/src/api.rs`

Four new handlers (`create_project`, `list_projects_handler`, `get_project_handler`, `update_project_handler`) mounted at:

```
POST   /api/v1/projects
GET    /api/v1/projects
GET    /api/v1/projects/:id
PATCH  /api/v1/projects/:id
```

— inside the same `protected` router that already applies `auth::require_auth`, so these routes are Bearer-gated exactly like every other endpoint. Authorization rules, each matching an existing precedent rather than inventing a new one:

| Action | Rule | Matches existing precedent |
|---|---|---|
| Create | `Role::Admin`, no additional scope restriction | `create_api_key` — an admin key, scoped or global, may already mint keys for any project |
| List | `ctx.project_id` filter, any authenticated role | `list_runs`, `list_bug_drafts` |
| Get | fetch-then-`require_project_access` | `get_run`, `list_evidence` |
| Update | `Role::Operator` + `require_project_access` | `create_run` (Operator bar), `approve_run`/`reject_run` (mutate-within-scope) |

Validation: `name` must be non-empty after trimming (400 otherwise) on both Create and Update. Both mutating actions call the existing `audit()` helper (`project.create`, `project.update`), so they appear in `/admin/audit-log` exactly like every other governed action — confirmed live (§6).

No response schema was invented beyond what §5.2 explicitly asks for ("the created project's stable identifier and authoritative metadata"): `{id, name, description, created_at, updated_at}` — no nested run counts or other Dashboard-adjacent fields (§23 is a separate, later concern).

---

## 5. Files Changed

| File | Change |
|---|---|
| `services/orchestrator/migrations/0004_projects.sql` | New — `projects` table |
| `services/orchestrator/src/store.rs` | New `Project` DTO + 4 functions (`insert_project`, `list_projects`, `get_project`, `update_project`) + `project_from_row` helper |
| `services/orchestrator/src/api.rs` | New request/response structs, 4 handlers, 2 new routes, `patch` import fix |
| `services/orchestrator/src/store_tests.rs` | 3 new tests (§6.1) |

No other file touched. No frontend file opened.

---

## 6. Tests Added

All three in `store_tests.rs`, real on-disk SQLite (`tempfile`), matching the exact convention every Phase 1 store test already uses:

- **`be8_project_create_and_get_round_trip`** — durable persistence of caller-supplied metadata plus server-assigned timestamps; `get_project` on an id that was never created returns `None`, not an error.
- **`be8_list_projects_respects_project_filter`** — `Some(pid)` returns exactly the matching project; `None` returns all; a filter naming a never-created id returns empty, not an error. **Discrimination-tested**: the `WHERE id = ?` clause was temporarily removed and the test re-run — it failed exactly as expected (`left: 2, right: 1`, i.e., the scoped filter leaking every project). The clause was then restored and the full suite reconfirmed green.
- **`be8_update_project_is_a_genuine_partial_update`** — updating only `name` leaves `description` untouched and vice versa (a prior partial update is not reverted by a later one); `updated_at` advances; updating a nonexistent id reports `false` (no row affected), which is what lets the API return 404 instead of a false "ok".

### 6.1 Before-fix / after-fix test result

Not applicable in the traditional "old buggy code" sense — this is new capability, not a bug fix. The equivalent, applied here: the isolation test (`be8_list_projects_respects_project_filter`) was proven **genuinely discriminating** by temporarily breaking the exact line it exists to protect and confirming it fails:

```
test store_tests::be8_list_projects_respects_project_filter ... FAILED
  assertion `left == right` failed: a scoped filter must return only the matching project
  left: 2
 right: 1
```

Restored, re-run: `3 passed; 0 failed`.

### 6.2 Live HTTP verification (real binary, real preserved-and-restored production database)

Every item on BE-8's own verification checklist was independently confirmed against a running instance, not inferred from the store tests alone:

| Check | Result |
|---|---|
| `/api/v1` routing (no key) | `401` |
| Viewer creates project | `403` |
| Operator creates project | `403` |
| Admin creates with blank name | `400` — `"project name must not be empty"` |
| Admin (global) creates project | `201`, full metadata returned |
| Admin (project-scoped) creates a *different* project | `201` — unrestricted, matching `create_api_key` |
| Key scoped to a project GETs its own project | `200` |
| Key scoped to a project GETs a *different* project | `403` — cross-project |
| GET a genuinely nonexistent project id | `404` — `"project not found"` |
| Scoped key LISTs | `200`, count = 1 (only its own) |
| Global admin LISTs | `200`, count ≥ 2 (all) |
| Viewer PATCHes | `403` — role |
| Operator scoped to project A PATCHes project B | `403` — cross-project |
| Operator scoped to project A PATCHes project A, `description` only | `200`; `name` unchanged, `description` updated, `updated_at` advanced |
| PATCH with blank name | `400` |
| PATCH a genuinely nonexistent project | `404` |
| **Regression:** `GET /health` | `200` |
| **Regression:** `GET /runs` (existing, unprefixed) | `200`, unaffected |
| **Regression:** `POST /runs` (existing, unprefixed) | `201`, unaffected |
| **Regression:** `GET /bug-drafts` | `200`, unaffected |
| Audit trail | `GET /admin/audit-log` shows `project.create` ×2 and `project.update` ×1 with correct actor/target/outcome |

Migration applied cleanly against the real, preserved production database with zero errors (`_sqlx_migrations` shows versions `[1, 2, 3, 4]` after boot). Database was restored to its exact pre-unit state afterward (migration 0004 will reapply automatically and safely on next boot, the same idempotent mechanism already relied on since Phase 1).

---

## 7. Phase-1 Regression

Re-run fresh after implementation:

| Suite | Result |
|---|---|
| D1 | 4 passed |
| D2 | 1 passed |
| D3 | 1 passed |
| D4 | 3 passed |
| D5 | 4 passed |
| B2 | 3 passed |
| B3 | 1 passed |
| B4 | 1 passed |
| B5 | 1 passed |
| B6 | 2 passed |
| B14 | 2 passed |

All unchanged, all passing. No Phase 1 file's *behavior* was modified — `api.rs` gained new routes/handlers only; `store.rs` gained new functions only; nothing existing was altered.

---

## 8. Build Result

```
$ cargo check --all-targets
    Checking aiqa-orchestrator v0.1.0 (D:\AIQA\services\orchestrator)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.02s
```
**0 errors, 0 warnings.**

```
$ cargo test
running 40 tests
... (37 pre-existing + 3 new be8_* tests, all ok)
test result: ok. 40 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.33s
```

---

## 9. Contract Impact

Net-new alignment: 4 of the contract's 61 endpoints (§5.1–5.4) now have a working, isolated implementation at the correct path. No existing contract-mismatch was worsened; the three pre-existing, already-documented mismatches (unprefixed legacy routes, unstructured errors, `INFRA_ERROR` naming) are untouched — none of them appear anywhere in the Project resource, since it's entirely new surface.

---

## 10. Database Impact

One additive migration (`0004_projects.sql`), one new table, zero changes to any existing table, zero new columns on `runs`/`api_keys`/etc., zero new foreign keys, zero backfill. Applied and verified live against the real production database with a full restore afterward. Existing `project_id` string-based isolation (`require_project_access`) is untouched and continues to function exactly as Phase 1 left it, independent of whether a `projects` row exists for any given string.

---

## 11. Frontend Impact

**None.** No file under `apps/web/` or `forentend/` was opened, read, or modified this session. `git status` for both paths shows the identical count of pre-existing (pre-BE-8) entries.

---

## 12. Remaining Issues (not addressed this unit — listed only)

- **Moving the 16 existing routes under `/api/v1/projects/{id}/...`** — not started; scope resolution in §1.
- **§5.5 Project Context** — deferred; depends on Phase 5 context ingestion.
- **FK / backfill from `runs.project_id` (and siblings) to the new `projects` table** — deferred; scope resolution in §1. Existing isolation is unaffected in the meantime.
- **Structured error envelope (§2.5)** — this unit's errors remain plain `(StatusCode, String)`, identical in shape to every existing endpoint. Explicitly this session's next unit.
- **No `DELETE /api/v1/projects/{id}`** — not in Contract §5 (only List/Create/Get/Update are specified there), so not implemented; not a gap against the contract as written.
- **Global-key list semantics** — resolved for this resource as "sees all projects," matching the existing convention used by every other list endpoint in the codebase; stated explicitly here since the Backend Implementation Plan flagged this as an open question needing an explicit decision.

---

## Final Status

# BE-8 — COMPLETE
