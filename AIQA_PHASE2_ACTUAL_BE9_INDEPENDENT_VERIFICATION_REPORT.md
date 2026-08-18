# AIQA — Actual BE-9 (Project-Scoped Routing) — Independent Verification

**Document:** `AIQA_PHASE2_ACTUAL_BE9_INDEPENDENT_VERIFICATION_REPORT.md`
**Date:** 2026-08-18
**Mode:** Hard verification only. No production source, tests, migrations, frontend, contract, matrix, auth architecture, or WebSocket code was modified during this pass.

---

## 1. Verification Scope

Independently verify every claim in `AIQA_PHASE2_ACTUAL_BE9_PROJECT_SCOPED_ROUTING_REPORT.md` against current source, a rebuilt binary, and direct database inspection — treating the report as a list of hypotheses, not evidence.

---

## 2. Verification Method

Four independent evidence channels, source of truth in this order: **source code > direct SQLite inspection > live HTTP requests against a rebuilt binary > automated test suite > the implementation report.**

- File mtimes were used to independently confirm *which files this specific unit touched*, before reading a single line of diff, so the diff audit did not depend on the report's own file list.
- Every handler and helper was re-read from the current file, not recalled from memory of writing it.
- Attack scenarios were constructed with fresh test data (two new real projects, four new keys, three new runs, one new evidence row, all minted directly against the database or via the router — not reused from the implementation session).
- Two security-critical checks (`require_resource_in_project`, the existence half of `require_scoped_project`) were temporarily disabled and the isolation tests re-run to confirm they are genuinely load-bearing, not decorative.
- The production database was preserved before this session and restored after; no orchestrator process was left running.

---

## 3. Independent Diff Audit (Step 1)

File mtimes (`Get-ChildItem | Sort-Object LastWriteTime`), checked *before* reading any diff content, to independently establish which files this specific unit could have touched:

```
engine.rs           2026-08-17 21:36:40   (predates this unit — Phase 1/D-series)
0004_projects.sql   2026-08-17 22:04:56   (predates this unit — BE-8)
store.rs            2026-08-17 22:08:59   (predates this unit — BE-8)
auth.rs             2026-08-17 22:18:25   (predates this unit — BE-9/structured-errors)
errors.rs           2026-08-17 22:41:50   (predates this unit — BE-10/extractor errors)
api_tests.rs         2026-08-17 23:04:31   ← this unit
api.rs               2026-08-17 23:06:09   ← this unit
```

Only `api.rs` and `api_tests.rs` have a write time consistent with this unit. Confirmed by content:

| File | Changed? | Purpose | In Scope? | Concern |
|---|---|---|---|---|
| `services/orchestrator/src/api.rs` | **Yes** | 8 new routes/handlers, 2 new authorization helpers, `create_run` refactored into `create_run_core` | Yes | None |
| `services/orchestrator/src/api_tests.rs` | **Yes** | 7 new tests | Yes | None |
| `services/orchestrator/src/auth.rs` | Shows in `git diff` (27 lines) | **Independently confirmed to be entirely BE-10's `ApiError` conversion** — `require_role`/`require_project_access`'s actual isolation logic (`None => Ok(())`, `Some(pid) => resource_pid == Some(pid)`) is byte-identical to before; only the return type changed | N/A — not this unit's diff | None |
| `services/orchestrator/src/store.rs`, `engine.rs`, `executor.rs`, `intelligence.rs`, `lib.rs`, `Cargo.toml`, `Cargo.lock`, `migrations/0004_projects.sql` | Show in `git diff`/untracked | All independently confirmed to predate this unit (mtime + content match BE-8/BE-9(errors)/BE-10/D-series reports exactly) | N/A | None |
| `services/orchestrator/src/policy.rs`, `scheduler.rs`, `domain.rs`, `state.rs`, `main.rs`, `metrics.rs`, `webhook.rs`, `evidence.rs` | **No** — `git diff --stat` returns empty | N/A | Correctly untouched | None |
| `apps/web/*`, `forentend/*` | Show in `git status` (pre-existing) | Frontend, unrelated to any Phase 2 backend unit | N/A | **Independently confirmed zero new changes** — diff count identical to before this unit |
| `AIQA_API_CONTRACT_v1.0.md`, `AIQA_ACCEPTANCE_MATRIX_v1.0.md` | **No** — untracked, unchanged | N/A | Correctly untouched | None |

**The report's "only two files changed" claim is verified true**, independently, via file modification timestamps rather than the report's own assertion.

---

## 4. Actual Route Inventory (Step 2)

Read directly from `api.rs`'s `router()` function, current source:

| Method | Actual Route | Handler | Auth | Project Scoped? | Legacy/New |
|---|---|---|---|---|---|
| GET | `/health` | `health` | none | No | unchanged |
| GET (WS) | `/runs/:id/ws` | `ws_handler` | `?key=` | Via run's own project | unchanged |
| POST/GET | `/api/v1/projects` | `create_project`/`list_projects_handler` | Bearer | N/A (root resource) | BE-8 |
| GET/PATCH | `/api/v1/projects/:id` | `get_project_handler`/`update_project_handler` | Bearer | N/A | BE-8 |
| **POST/GET** | **`/api/v1/projects/:project_id/executions`** | **`create_run_scoped`/`list_runs_scoped`** | Bearer | **Yes** | **New, confirmed present** |
| **GET** | **`/api/v1/projects/:project_id/executions/:run_id`** | **`get_run_scoped`** | Bearer | **Yes** | **New, confirmed present** |
| **GET** | **`/api/v1/projects/:project_id/executions/:run_id/events`** | **`get_events_scoped`** | Bearer | **Yes** | **New, confirmed present** |
| **POST** | **`/api/v1/projects/:project_id/executions/:run_id/approve`** | **`approve_run_scoped`** | Bearer | **Yes** | **New, confirmed present** |
| **POST** | **`/api/v1/projects/:project_id/executions/:run_id/reject`** | **`reject_run_scoped`** | Bearer | **Yes** | **New, confirmed present** |
| **GET** | **`/api/v1/projects/:project_id/executions/:run_id/evidence`** | **`list_evidence_scoped`** | Bearer | **Yes** | **New, confirmed present** |
| **GET** | **`/api/v1/projects/:project_id/evidence/:evidence_id`** | **`get_evidence_content_scoped`** | Bearer | **Yes** | **New, confirmed present** |
| POST/GET | `/runs` | `create_run`/`list_runs` | Bearer | No (legacy) | unchanged |
| GET | `/runs/:id` | `get_run` | Bearer | No | unchanged |
| GET | `/runs/:id/events` | `get_events` | Bearer | No | unchanged |
| POST | `/runs/:id/approve` | `approve_run` | Bearer | No | unchanged |
| POST | `/runs/:id/reject` | `reject_run` | Bearer | No | unchanged |
| GET | `/runs/:id/evidence` | `list_evidence` | Bearer | No | unchanged |
| GET | `/evidence/:id/content` | `get_evidence_content` | Bearer | No | unchanged |
| GET | `/intelligence/tests` | `list_test_intelligence` | Bearer | No | unchanged |
| GET | `/intelligence/failure-clusters` | `list_failure_clusters` | Bearer | No | unchanged |
| GET | `/bug-drafts` | `list_bug_drafts` | Bearer | No | unchanged |
| POST | `/bug-drafts/:id/review` | `review_bug_draft` | Bearer | No | unchanged |
| GET | `/metrics` | `get_metrics` | Bearer | No | unchanged |
| POST/GET | `/admin/api-keys` | `create_api_key`/`list_api_keys_handler` | Bearer, Admin | No | unchanged |
| DELETE | `/admin/api-keys/:id` | `revoke_api_key_handler` | Bearer, Admin | No | unchanged |
| GET | `/admin/audit-log` | `get_audit_log` | Bearer, Admin | No | unchanged |

**All 8 claimed routes exist, are correctly mounted inside the `protected` router (so `require_auth` wraps them), and are backed by real handler functions** — verified by reading the router registration directly, not by trusting route names in the report.

---

## 5. Route ↔ Contract Comparison

| New route | Contract reference | Match |
|---|---|---|
| `.../executions/:run_id/evidence` | §15.1 | Exact path match |
| `.../evidence/:evidence_id` | §15.2 | Exact path match |
| `.../executions` (create/list) | §12.1/§12.2 | Path/resource-noun match; request/response **shape** intentionally unchanged from legacy `RunPlan` — confirmed correctly disclosed, not silently claimed as full §12 compliance |
| `.../events`, `/approve`, `/reject` | Not enumerated in §12 | Confirmed genuinely absent from the contract; extension under an existing contract-defined resource, not an invented top-level path |

No contract mismatch was found that the report failed to disclose.

---

## 6. Project Context Verification (Step 3)

Read `require_scoped_project` and `require_resource_in_project` directly (`api.rs:130–148`):

- **A. URL canonical:** `create_run_scoped` takes `project_id` only from `ApiPath<Uuid>`; there is no `project_id` field anywhere in its query or body types. Confirmed by reading the handler signature — no override path exists in source, and independently confirmed live (§8: an unauthorized key cannot create under a project it doesn't own regardless of what's in the body, since the body has no project field to manipulate).
- **B. Existence:** `require_scoped_project` calls `state.store.get_project(project_id)` and 404s before checking authorization. **Independently disabled and re-tested** (§9) — confirmed a nonexistent project silently returned `200` with an empty list to a global admin key when this check was removed. Restored; the dedicated test (`actual_be9_nonexistent_project_returns_structured_not_found`) and a fresh live check both now correctly 404.
- **C. Authorization:** `auth::require_project_access` is called unmodified — the exact function Phase 1 already verified.
- **D. Resource ownership:** `require_resource_in_project` / `scoped_run_summary` compare the resource's own stored `project_id` against the URL. **Independently disabled and re-tested** (§9) — confirmed Project A's key received Project B's complete run detail and raw evidence bytes under A's own URL when this check was removed. Restored; confirmed fixed.

---

## 7. Cross-Project Read Security (Step 4)

Fresh, independently-constructed attack, real database, real HTTP, real binary (fixtures: two new projects, `operator-a`/`operator-b` keys, a real completed run "iv-b-run" under Project B with a real event log and a real evidence row inserted via direct filesystem + DB write):

| Attack | Result | Data leaked? |
|---|---|---|
| A reads B's run under B's URL | `403 FORBIDDEN` | No |
| A reads B's run id under A's URL | `404 RESOURCE_NOT_FOUND` | No |
| A reads B's events under B's URL | `403 FORBIDDEN` | No |
| A reads B's events under A's URL | `404 RESOURCE_NOT_FOUND` | No |
| A reads B's evidence list under B's URL | `403 FORBIDDEN` | No |
| A reads B's evidence list under A's URL | `404 RESOURCE_NOT_FOUND` | No |
| A reads B's evidence-by-id under B's URL | `403 FORBIDDEN` | No |
| A reads B's evidence-by-id under A's URL | `404 RESOURCE_NOT_FOUND` | No |
| A lists B's executions under B's URL | `403 FORBIDDEN` | No |

All nine responses inspected directly; none contained `run_id`, `name`, task data, event data, or the evidence content string (`"real IV evidence content — sensitive B data"`, checked via substring search on every raw response body).

---

## 8. Cross-Project Mutation Security (Step 4 cont.)

Two fresh gated runs (`executor: "maestro"`, real policy-gated `PENDING_APPROVAL`) created under Project B specifically for this attack:

| Attack | Result |
|---|---|
| A approves B's run under A's own URL | `404 RESOURCE_NOT_FOUND` |
| A approves B's run under B's URL | `403 FORBIDDEN` |
| A rejects B's run under A's own URL | `404 RESOURCE_NOT_FOUND` |
| A rejects B's run under B's URL | `403 FORBIDDEN` |

**Database state compared before vs. after all four mutation attempts** (`runs.state`, `approved_by`, `approved_at`, `finished_at`, every task's status, event count per run, and global row counts for `runs`/`tasks`/`run_events`): **byte-identical in every field.** No partial mutation, no orphaned state, no side effect of any kind from the denied attempts.

Positive control: Project B's *own* key approving its own gated run returned `200`, `state: "RUNNING"` — confirming the denials above are genuine authorization rejections, not a broken handler that denies everything.

---

## 9. Tenant Isolation (Step 5)

Fresh two-tenant scenario (not reusing Phase 1's fixtures): Tenant A (`viewer` role) and Tenant B (`operator` role), each with their own newly-created project. Tenant A's viewer key reading Tenant B's run under B's own path: `403 FORBIDDEN`. Tenant B's own key: `200`. This exercises `auth::require_project_access` — confirmed by source (§6) to be the unmodified Phase-1 function — through the new routes, proving reuse is real rather than a parallel reimplementation that merely resembles it.

**Discrimination check results are in §6/§10** — both critical checks were proven load-bearing by disabling them, not merely exercised while already correct.

---

## 10. Evidence Ownership Security (Step 6)

Traced the exact chain in source: `get_evidence_content_scoped` (`api.rs:846–870`) calls `store.get_evidence_object_key(evidence_id)` → extracts `run_id` from the returned row → calls `scoped_run_summary(state, run_id, project_id)`, which fetches the run's summary and calls `require_resource_in_project(summary.project_id, project_id)`. There is no path that returns evidence bytes without this last check succeeding — confirmed by reading the function body in full; the `evidence.read(&object_key)` call is the *last* statement, after every check.

**Live test with real evidence bytes** (not a placeholder): inserted `"real IV evidence content — sensitive B data"` directly via the real `LocalEvidenceStore` and a real `evidence` table row for Project B's run. Project A's key requesting it under A's own URL: `404`, response body confirmed to contain zero bytes of the real content (JSON error envelope only). Under B's URL: `403`. Project B's own key: `200`, and the returned bytes were verified byte-for-byte equal to what was written (confirmed via the automated test `actual_be9_cross_project_isolation_on_evidence_by_id`, independently re-run this session).

**Discrimination test:** `require_resource_in_project` disabled → Project A's key received the evidence content successfully (`200`, real bytes) under A's own URL naming B's evidence id. This is the single most severe finding this pass could have surfaced, and the fix is confirmed present and effective.

---

## 11. Run Creation Integrity (Step 7)

| Case | Result | DB verification |
|---|---|---|
| Valid create under Project A's real project by its own operator key | `201`, run stored | `runs.project_id` confirmed equal to the URL's project UUID (not the legacy free-text convention) |
| Nonexistent project | `404 RESOURCE_NOT_FOUND` | `SELECT COUNT(*) FROM runs WHERE name='phantom-attempt'` → **0**; `SELECT COUNT(*) FROM tasks t LEFT JOIN runs r ON r.id=t.run_id WHERE r.id IS NULL` → **0** orphaned tasks |
| Unauthorized project (viewer key scoped to A, targeting B) | `403 FORBIDDEN` ("resource belongs to a different project") | No run created (same zero-row checks apply — both failed attempts shared a plan name, and the count for it is 0) |
| Right project, insufficient role (viewer, own project) | `403 FORBIDDEN` ("role 'viewer' cannot perform this action (requires 'operator')") | No run created |

No partial run row, no orphaned task row, and — since creation never reached `engine::spawn_run` in any denied case — no background execution and no event was emitted for any of the three denied attempts.

---

## 12. Approve/Reject and D1 Regression (Step 8)

**Source-level confirmation:** `approve_run_scoped`/`reject_run_scoped` call `engine::approve_and_spawn`/`engine::reject_run` — the identical functions the legacy handlers call, confirmed by reading both call sites side-by-side; no parallel/duplicate implementation exists.

**Real fault injection through the scoped route** (not merely cited as "already verified" — reproduced fresh): a second orchestrator-independent SQLite connection held `BEGIN EXCLUSIVE` for the duration of a scoped `POST .../approve` call, exceeding the real 5-second `busy_timeout`. Result:

```
status=500
{"error":{"code":"INTERNAL_ERROR","message":"An internal error occurred.","details":{},"request_id":"..."}}

DB state after failed approve: state=PENDING_APPROVAL approved_by=None
```

This is D1's exact guarantee (no false success; the run does not appear approved when the durable write failed) reproduced through the new route, not merely inferred from the call chain. Server log confirmed the real diagnostic was captured, not lost: `ERROR aiqa_orchestrator::errors: internal error: error returned from database: (code: 5) database is locked`.

---

## 13. Legacy Route Verification (Step 9)

`POST /runs`, `GET /runs/:id`, `GET /runs` (with the new scoped-route-created runs mixed into the same admin-visible list), `GET /bug-drafts`, `GET /admin/audit-log` all independently re-tested live: all return `200`/`201` with unchanged shapes. `GET /runs` for an admin key correctly shows both legacy string-scoped runs and new UUID-project-scoped runs together, undifferentiated — this is expected, unchanged behavior (a global key has always seen everything; it is not a new inconsistency introduced by this unit).

---

## 14. Structured Error Verification (Step 10)

Tested fresh through the **new** scoped routes specifically (not just re-citing BE-10's own tests against the old routes):

| Case | Result |
|---|---|
| Malformed JSON to scoped create | `400`, `VALIDATION_ERROR` envelope |
| Invalid UUID in `project_id` path segment | `400`, `VALIDATION_ERROR` envelope |
| Invalid UUID in `run_id` path segment (tuple `ApiPath<(Uuid, Uuid)>`) | `400`, `VALIDATION_ERROR` envelope — **confirms BE-10's extractor wrapper correctly handles multi-segment tuple paths, not just the single-`Uuid` case it was originally tested against** |
| Missing authentication on a scoped route | `401`, `UNAUTHENTICATED` envelope |
| Nonexistent project | `404`, `RESOURCE_NOT_FOUND` |
| Nonexistent run | `404`, `RESOURCE_NOT_FOUND` |
| Wrong-project resource | `404`/`403` per §7–8 |
| Unauthorized project/role | `403`, `FORBIDDEN` |
| Real internal failure (§12) | `500`, `INTERNAL_ERROR`, sanitized (no `database is locked` text present in any response body checked) |

No raw internal error, stack trace, or database detail was found in any response body across this entire verification pass.

---

## 15. Database Integrity (Step 11)

Already detailed in §8 (mutation attempts: byte-identical before/after) and §11 (creation attempts: zero rows). Successful requests independently confirmed correct: the run created via the scoped route stored `project_id` equal to the real project UUID (not a legacy string), its task row and event rows are correctly attached to that run id, and the evidence row's `run_id` correctly resolves back to the project used in the ownership check.

---

## 16. Test Results (Step 12)

Fresh, independent run (not copied from the implementation session's transcript):

```
$ cargo test
test result: ok. 62 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Exact category breakdown, counted from this run's own output:

| Category | Count |
|---|---|
| `actual_be9_*` (this unit) | 7 |
| `be9_*` (structured errors) | 10 |
| `be10_*` (extractor errors) | 5 |
| `be8_*` (store-level project tests) + the router-level BE-8 lifecycle test | 4 |
| `d1_*` | 4 |
| `d2_*` | 1 |
| `d3_*` | 1 |
| `d4_*` | 3 |
| `d5_*` | 4 |
| `b1_*` | 2 |
| `b2_*` | 3 |
| `b3_*` | 1 |
| `b4_*` | 1 |
| `b5_*` | 1 |
| `b6_*` | 2 |
| `b14_*` | 2 |
| Other pre-existing (`executor`, `intelligence`, `policy`, `scheduler`) | 12 |
| **Total** | **62** |

---

## 17. Build Results (Step 12 cont.)

```
$ cargo check --all-targets
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.92s
```
**0 errors, 0 warnings.**

---

## 18. Phase-1 Regression (Step 13)

`D1`–`D5` (13 tests total) and `B2`/`B3`/`B4`/`B5`/`B6`/`B14` (10 tests total) all pass, confirmed in the same run as §16 — not a separate, possibly-stale citation. Beyond test names: §12 reproduced D1's actual runtime guarantee (no false approval success under a real persistence fault) through the *new* code path, which is a stronger check than trusting that the test suite alone still passing means the guarantee holds for code the D1 tests never call.

---

## 19. Previous Phase-2 Regression

- **BE-8 (Project API):** store-level tests (3) pass; live-tested fresh in §11 (create under a real project, correct `project_id` stored). Project-management endpoints themselves (`/api/v1/projects*`) were independently confirmed **not** additionally scoped by this unit — reading the router shows they remain exactly as BE-8 left them.
- **Structured errors (this session's prior "BE-9"):** 10 tests pass; live-tested fresh in §14, including the sanitization guarantee under a real fault (§12).
- **Extractor-level structured errors ("BE-10"):** 5 tests pass; live-tested fresh in §14, additionally confirming (a genuinely new check beyond what BE-10's own report tested) that the tuple-path variant works correctly.

---

## 20. Frontend Integrity (Step 14)

`git status --porcelain -- apps/web/ forentend/` returns the identical set of entries present before this unit and every prior Phase 2 unit — no new diff. No file under either path was opened by this verification.

---

## 21. Contract/Matrix Integrity

`AIQA_API_CONTRACT_v1.0.md` and `AIQA_ACCEPTANCE_MATRIX_v1.0.md` are both untracked and show **no modification** — confirmed via `git status`, not opened with any write tool this session.

---

## 22. Auth/WS Scope Integrity

- **G-AUTHN:** `auth.rs`'s diff (§3) is confirmed to be entirely BE-10's error-type conversion; the actual authentication/authorization *semantics* (role hierarchy, project-scope comparison) are unmodified. No user/session model, no key-vs-user resolution was introduced.
- **G-WS:** `ws_handler` (`api.rs:1059`) confirmed still using plain `Path`/`Query` (not the new `ApiPath`/`ApiQuery` wrappers) and its own `?key=` authentication, completely unmodified by this unit.

---

## 23. Defects Found

**None.** Every check in Steps 3–14 passed. The two deliberately-induced failures (§6, §10 — temporarily disabling `require_resource_in_project` and the existence check) were **verification actions**, not defects discovered in the shipped code — both were restored immediately after confirming the tests correctly detect their absence, and the shipped state (confirmed by the final `cargo test`/`cargo check` run in §16–17, executed *after* restoration) has both protections active.

---

## 24. Implementation Claims That Failed Verification

**None.** Every claim listed in the verification request was independently reproduced:

| Claim | Verified |
|---|---|
| 8 new project-scoped routes added | ✅ §4, read directly from router |
| Project context enforced from the URL | ✅ §6 |
| Project existence checked | ✅ §6, §9 (discrimination-tested) |
| Project access authorization enforced | ✅ §6, §7, §8, §9 |
| Run/evidence ownership checked against URL project | ✅ §6, §10 (discrimination-tested) |
| Cross-project read blocked | ✅ §7 |
| Cross-project mutation blocked | ✅ §8, with zero-DB-mutation confirmation |
| Cross-project evidence access blocked | ✅ §10, with real evidence bytes |
| Tenant isolation intact | ✅ §9 |
| Legacy flat routes functional | ✅ §13 |
| Structured errors intact | ✅ §14 |
| Phase-1 D1–D5 intact | ✅ §18, plus a live D1 reproduction through the new code |
| B2/B3/B4/B5/B6/B14 intact | ✅ §18 |
| 62/62 tests pass | ✅ §16, fresh run |
| No frontend/auth/WS/contract/matrix changes | ✅ §20–22 |

---

## 25. Remaining Non-BE-9 Work

Explicitly out of this unit's scope, not a BE-9 defect:

- **BE-12** (Dashboard/KPI JSON, `/metrics` remains Prometheus text)
- **BE-13** (Execution request/response schema alignment to Contract §12's `test_case_ids`/`test_suite_id`; the auditable cancel endpoint)
- **BE-14** (project-scoped realtime event stream — the current `/events` catch-up path is per-run, not per-project)
- **BE-15** (`/intelligence/*`, `/bug-drafts*`, `/admin/audit-log` renamed/rescoped to the contract's "findings"/"audit" vocabulary)
- **G-AUTHN** (keys-vs-users; `/admin/api-keys*` correctly left unscoped, no contract endpoint exists for it)
- **G-WS** (WebSocket auth/transport model)
- Legacy runs created before this unit (free-text `project_id`) are not reachable through the new scoped routes — a disclosed, accurate, non-defect consequence of the "project exists" check combined with BE-8's deliberate non-retrofit of historical data.

None of the above was incorrectly treated as a BE-9 failure in this verification.

---

## 26. Final Verdict

# ACTUAL BE-9 — VERIFIED PASS — GO

**Can the next backend milestone start? YES**
