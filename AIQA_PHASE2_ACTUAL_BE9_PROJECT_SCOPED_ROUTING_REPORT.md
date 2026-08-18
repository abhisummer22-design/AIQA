# AIQA — Phase 2 — Actual BE-9: `/api/v1` + Project-Scoped Routing

**Document:** `AIQA_PHASE2_ACTUAL_BE9_PROJECT_SCOPED_ROUTING_REPORT.md`
**Date:** 2026-08-17
**Unit:** BE-9 (Integration Plan's original numbering) — `/api/v1` scoping + project-scoped routing
**Scope:** Backend only. Zero frontend files opened or modified.

---

## 1. Exact BE-9 Scope

Per `AIQA_INTEGRATION_IMPLEMENTATION_PLAN_v1.0.md` §15: `BE-9 | /api/v1 + scoped routing | All routes contract-shaped`.

Read together with the plan's own Phase-sequencing section (§13), "all routes contract-shaped" is not a claim BE-9 achieves alone — three *other*, separately-named, later milestones own specific sub-slices of it:

- `BE-13 | Execution + cancel | §12.1–12.5 incl. auditable cancel` — full request/response **schema** alignment to Contract §12 (which presupposes `test_case_ids`/`test_suite_id` entities this backend does not have) and the new cancel endpoint.
- `BE-14 | Project event stream | §14 with per-project sequence + resync-on-lag` — the realtime model itself.
- `BE-15 | Evidence/findings/audit on contract paths | §15, §16, §25` — bug-drafts→"findings" and audit-log naming/concept alignment.

**BE-9's own scope, therefore, is routing and project-scoping of the resources that don't require a deeper schema or naming change to relocate correctly**: the execution (run) lifecycle and its evidence, moved under `/api/v1/projects/{project_id}/...`, with their **existing** request/response shapes preserved exactly. This was determined from the documents before any code was written, not assumed.

---

## 2. Route Inventory (Step 1)

The 16 pre-existing routes (matching the Integration Plan's own "16 + 1 WS" count, `/health` included) plus the 1 WebSocket route:

| Current Route | Method | Target Route | Project Scoped? | Auth | Role | Contract §|
|---|---|---|---|---|---|---|
| `/health` | GET | *(unchanged — global)* | No | None | — | — |
| `/runs` | POST | `/api/v1/projects/{id}/executions` | **Yes — new route added** | Yes | Operator | §12.1 |
| `/runs` | GET | `/api/v1/projects/{id}/executions` | **Yes — new route added** | Yes | any | §12.1 (list) |
| `/runs/:id` | GET | `/api/v1/projects/{id}/executions/{run_id}` | **Yes — new route added** | Yes | any | §12.2 |
| `/runs/:id/events` | GET | `/api/v1/projects/{id}/executions/{run_id}/events` | **Yes — new route added** | Yes | any | not enumerated (extension) |
| `/runs/:id/approve` | POST | `/api/v1/projects/{id}/executions/{run_id}/approve` | **Yes — new route added** | Yes | Operator | not enumerated (extension) |
| `/runs/:id/reject` | POST | `/api/v1/projects/{id}/executions/{run_id}/reject` | **Yes — new route added** | Yes | Operator | not enumerated (extension) |
| `/runs/:id/evidence` | GET | `/api/v1/projects/{id}/executions/{run_id}/evidence` | **Yes — new route added** | Yes | any | §15.1 (exact match) |
| `/evidence/:id/content` | GET | `/api/v1/projects/{id}/evidence/{evidence_id}` | **Yes — new route added** | Yes | any | §15.2 (path match) |
| `/intelligence/tests` | GET | *(unchanged)* | No | Yes | any | No contract analogue — see §3 |
| `/intelligence/failure-clusters` | GET | *(unchanged)* | No | Yes | any | BE-15's scope (→"findings") |
| `/bug-drafts` | GET | *(unchanged)* | No | Yes | any | BE-15's scope (→"findings") |
| `/bug-drafts/:id/review` | POST | *(unchanged)* | No | Yes | Operator | BE-15's scope |
| `/metrics` | GET | *(unchanged)* | No | Yes | any | Not §23-shaped (Prometheus text); BE-12's scope, excluded from this task |
| `/admin/api-keys` | POST/GET | *(unchanged)* | No — deliberate | Yes | Admin | No contract endpoint exists; G-AUTHN-gated |
| `/admin/api-keys/:id` | DELETE | *(unchanged)* | No — deliberate | Yes | Admin | Same |
| `/admin/audit-log` | GET | *(unchanged)* | No | Yes | Admin | BE-15's scope (§25) |
| `/runs/:id/ws` | GET (WS) | *(unchanged)* | No | `?key=` | — | G-WS, explicitly excluded |

8 new routes added. 9 legacy routes left completely untouched (including `/health` and the WS route).

---

## 3. Target Routing Determination (Step 2)

**Moved (routing only, request/response shapes unchanged):** the 8 rows above marked "Yes — new route added". `/runs/:id/evidence` and `/evidence/:id/content` have **exact** Contract §15.1/§15.2 path matches. `/events`, `/approve`, `/reject` have no contract-enumerated path; they are mounted as sub-resources of `executions/{run_id}` — a reasonable extension of the resource the contract *does* define (Contract §2.1: "non-breaking additions may remain within the current version"), not an invented top-level structure.

**Not moved, and why (Step 2's explicit instruction: do not force-scope admin/system endpoints, do not invent structure):**

- **`/admin/api-keys*`** — the contract has **no API-key-management endpoint at all**. Key management is tied to the unresolved G-AUTHN decision (keys vs. users); scoping or moving it would mean inventing structure the contract doesn't define, explicitly forbidden.
- **`/metrics`** — the contract's nearest concept (§23 Dashboard) is a different endpoint, different shape (JSON KPIs vs. Prometheus text), assigned to `BE-12`, explicitly excluded from this task.
- **`/intelligence/*`, `/bug-drafts*`, `/admin/audit-log`** — genuine naming/concept mismatches against the contract's "Findings" (§16) and "Audit" (§25) vocabulary, explicitly assigned to `BE-15`, a separate, later milestone. Moving these now would mean guessing at a rename the source documents assign to different, unstarted work — Step 6's own instruction is to stop and report this rather than invent it, which is what happened here.

---

## 4. Project Context Enforcement (Step 3)

Every new route enforces, in this order, via two small shared functions (not duplicated per-handler):

```rust
async fn require_scoped_project(state, ctx, project_id) -> Result<(), ApiError> {
    // 1. the project in the URL must actually exist (store.get_project)
    // 2. the caller must be authorized for it (auth::require_project_access — reused, not reimplemented)
}

fn require_resource_in_project(resource_project_id, url_project_id) -> Result<(), ApiError> {
    // 3. the specific resource (run/evidence) fetched must itself belong to the URL's project
}
```

The URL's `project_id` is canonical everywhere. The legacy `create_run`'s `?project_id=` query-parameter fallback (for global keys) is **not present at all** on the new `create_run_scoped` handler — it has no `project_id`-bearing query or body field; the only source is the path.

---

## 5. Files Changed

| File | Change |
|---|---|
| `services/orchestrator/src/api.rs` | 8 new routes + 8 new handlers + 2 shared authorization helpers + `create_run` refactored into a shared `create_run_core` (used by both the legacy and new handler, avoiding duplicated business/policy logic) |
| `services/orchestrator/src/api_tests.rs` | 7 new tests (§8) |

No other file touched. No migration, no frontend file, no `auth.rs` change (its BE-9(structured-errors)/BE-10 diff is untouched by this session).

---

## 6. Handler / Authorization Changes

- `require_scoped_project` and `require_resource_in_project` are the only new authorization primitives. Both are thin compositions of the **existing** `auth::require_role`/`auth::require_project_access` — the security model itself was not touched, matching Step 7's explicit instruction.
- `create_run_core` factors the run-creation logic (validation, policy gate, persistence, spawn) out of `create_run` so `create_run_scoped` calls the identical code path with a different `project_id` source — zero duplicated business logic.
- `get_run_scoped`/`get_events_scoped`/`list_evidence_scoped` share a `scoped_run_summary` helper that performs the two-tier (in-memory-then-DB) lookup every legacy handler already used, plus the new ownership check, written once.
- `approve_run_scoped`/`reject_run_scoped` reuse `engine::approve_and_spawn`/`engine::reject_run` — the exact same Phase-1-verified engine functions the legacy handlers call, unmodified.

---

## 7. Project Isolation Tests (Step 4)

All in `api_tests.rs`, real router, real on-disk SQLite, real projects created via `store.insert_project`:

- **`actual_be9_cross_project_isolation_is_enforced_on_every_scoped_route`** — Project A cannot read Project B's run (`GET`), events, evidence-list, or run-list under B's own path (**403**, wrong namespace) — and cannot reach the same resources by naming B's run id under A's *own* path either (**404**, genuinely not in A's namespace). Also proves A cannot mutate (approve) B's run either way.
- **`actual_be9_cross_project_isolation_on_evidence_by_id`** — the same, for `GET .../evidence/{evidence_id}` specifically, which resolves ownership through the evidence's *owning run* rather than a direct column — real evidence bytes inserted via the real store and evidence filesystem, confirmed unreadable cross-project.
- **`actual_be9_nonexistent_project_returns_structured_not_found`** — a project id that was never created 404s, for both GET and POST, even for a global admin key.

**Discrimination-verified (Step 9 test-first), not merely passing by construction:**

1. `require_resource_in_project` was temporarily disabled (made an unconditional `Ok(())`). Result: Project A's key received Project B's **complete run detail** (name, tasks, `project_id`) under A's own URL — a real, concrete data leak, not a hypothetical:
   ```
   left: 200
   right: 404
   body={"project_id":"...B's real id...","name":"b-run","tasks":[...],...}
   ```
   The evidence-by-id test failed the same way (A received B's raw evidence bytes). Both confirm the check is load-bearing, not decorative. Restored; both tests pass again.
2. `require_scoped_project`'s existence check was temporarily disabled. Result: an admin key received a silent `200` empty list for a project UUID that was **never created** — confirmed via the dedicated non-existence test failing (`left: 200, right: 404`). Restored.

---

## 8. Tenant Isolation Tests (Step 5)

**`actual_be9_tenant_isolation_preserved_through_scoped_routes`** — two projects treated as distinct tenants; a viewer-role key scoped to Tenant A is rejected (**403**, `FORBIDDEN` code) reading Tenant B's run under B's own path; Tenant B's own key succeeds. This exercises `auth::require_project_access` — the exact same function Phase 1's B3/B14 tests already proved correct — through the new routes, proving that reuse is real rather than a reimplementation that merely resembles it.

---

## 9. Real HTTP Verification (Step 10)

Rebuilt binary, real (preserved-and-restored) production database, real network requests, two genuine projects and their scoped keys minted directly against the running database:

| Check | Result |
|---|---|
| Create a run under Project B via the scoped route | `201`, `{"run_id":...,"state":"SCHEDULED",...}` |
| Project A's key reads Project B's run under B's path | `403` — `{"error":{"code":"FORBIDDEN","message":"resource belongs to a different project",...}}` |
| Project A's key reads Project B's run **id** under A's own path | `404` — `{"error":{"code":"RESOURCE_NOT_FOUND","message":"resource not found in this project",...}}` |
| Project B's key reads its own run | `200`, correct `project_id` in the body |
| Nonexistent project | `404`, `RESOURCE_NOT_FOUND` |
| Legacy `GET /runs` | `200`, unaffected, still lists everything an admin key can see |
| BE-8 regression: create/get a project | `201` / `200`, unaffected |
| BE-10 regression: malformed JSON to `/api/v1/projects` | `400`, `VALIDATION_ERROR` envelope, unaffected |

Every result matches the automated test suite exactly.

---

## 10. Legacy Route Behavior (Step 6)

**No document specifies removal, redirection, or deprecation** of the flat routes. Per Step 6's explicit "if ambiguous, stop and report" instruction, this is the resolved ambiguity: the flat routes **remain, fully functional, unchanged**, alongside the new scoped ones. Rationale:

1. No source document instructs removal — inventing a deletion/redirect policy would itself be "inventing behavior."
2. This exact "add new, keep old" pattern was already used safely three times in this engagement (BE-8, BE-9/structured-errors, BE-10), with zero regressions each time.
3. Removing or redirecting the legacy routes would be a materially riskier, harder-to-reverse action (breaking anything that currently depends on them) for no documented requirement.

`actual_be9_legacy_flat_routes_remain_fully_functional` proves this directly: `POST /runs`, `GET /runs/:id`, `GET /runs` all still work exactly as before.

---

## 11. BE-8 Regression (Step 12)

```
be8_*: 4 passed (store-level)
```
Plus live verification (§9): project create and get both still return `201`/`200` with the unchanged schema. Project-management endpoints (`/api/v1/projects`, `/api/v1/projects/:id`) were **not** touched or additionally scoped by this unit — they were already correctly designed as the root-level resource one level *above* what BE-9 scopes other things by.

---

## 12. BE-10 Regression (Step 13)

```
be10_*: 5 passed
```
Plus live verification (§9): malformed JSON to the (untouched) `/api/v1/projects` route still returns the `VALIDATION_ERROR` envelope, confirming `ApiJson`/`ApiPath`/`ApiQuery` continue working correctly and were reused (not reimplemented) for every new handler in this unit.

---

## 13. Phase 1 (D1–D5) Regression

```
D1 — 4 passed
D2 — 1 passed
D3 — 1 passed
D4 — 3 passed
D5 — 4 passed
```
All unchanged — none of D1–D5's mechanisms were touched; `engine::approve_and_spawn`/`engine::reject_run`/`store.insert_run` are called identically by both legacy and new handlers.

---

## 14. B2/B3/B4/B5/B6/B14 Regression

```
B2  — 3 passed
B3  — 1 passed
B4  — 1 passed
B5  — 1 passed
B6  — 2 passed
B14 — 2 passed
```
All unchanged.

---

## 15. Full Test Suite

```
$ cargo test
running 62 tests
... (55 pre-existing + 7 new actual_be9_* tests)
test result: ok. 62 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.5x s
```
Re-run three times total during this unit (once clean, twice more after the two discrimination-revert cycles were restored): identical 62/62 every time.

---

## 16. Build Result

```
$ cargo check --all-targets
    Checking aiqa-orchestrator v0.1.0 (D:\AIQA\services\orchestrator)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 1.5x s
```
**0 errors, 0 warnings.** The router and every new handler compiled cleanly on the first attempt after the shared-helper design was settled.

---

## 17. Contract Verification

Every new route was checked against `AIQA_API_CONTRACT_v1.0.md` before implementation (§2–3 above). Two exact path matches (§15.1, §15.2); the execution paths follow the contract's own resource noun ("executions") and nesting under `/api/v1/projects/{project_id}/` without adopting its request/response schema, which is explicitly `BE-13`'s later, separate scope. `AIQA_API_CONTRACT_v1.0.md` itself was not modified — confirmed via `git status` (untracked, unchanged) throughout.

---

## 18. Acceptance Matrix Verification

`AIQA_ACCEPTANCE_MATRIX_v1.0.md` was not modified — confirmed via `git status` (untracked, unchanged). No ACC item's recorded status was touched by this unit.

---

## 19. Frontend Verification

**None touched.** No file under `apps/web/` or `forentend/` was opened, read, or modified this session (`git status` diff count for both paths identical to before this unit started).

---

## 20. Remaining Issues

- **Request/response schema alignment for executions (Contract §12's `test_case_ids`/`test_suite_id`/`execution_mode`) and the cancel endpoint (§12.3)** — explicitly `BE-13`, not attempted.
- **`/intelligence/*`, `/bug-drafts*`, `/admin/audit-log` → contract-shaped "findings"/"audit" paths** — explicitly `BE-15`, not attempted; these remain flat and unscoped.
- **`/admin/api-keys*` scoping** — correctly not attempted; no contract endpoint exists, and doing so would require resolving G-AUTHN first.
- **`/metrics` → `/dashboard`/`/kpis` JSON** — explicitly `BE-12`, not attempted.
- **Legacy runs created before this unit** (with free-text `project_id` values like `"alpha"`/`"p1"`/`"default"` rather than a real `projects` UUID) are **not reachable** through the new scoped routes — only through the legacy flat ones. This is an accurate, disclosed consequence of `require_scoped_project`'s real "project exists" check (Step 3's own requirement) combined with BE-8's deliberate decision not to retrofit a foreign key onto historical `project_id` values. New runs created via the new scoped route are properly linked to a real project from creation onward.

---

## 21. Authentication Dependency

None introduced. `require_auth`, `require_role`, `require_project_access` are unmodified. `G-AUTHN` remains unresolved and was not approached — nothing in this unit's actual scope (routing existing functionality) required resolving it.

---

## 22. WebSocket Dependency

None introduced. `ws_handler` and its `?key=` authentication are completely untouched. `G-WS` remains unresolved and was not approached.

---

## Final Status

# ACTUAL BE-9 — COMPLETE
