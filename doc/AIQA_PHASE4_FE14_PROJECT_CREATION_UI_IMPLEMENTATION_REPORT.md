# AIQA — Phase 4 — FE-14 Implementation Report

## 1. Status

**PASS.**

## 2. Objective

Implement a real Project Creation UI, closing GAP-01 (the sole audit-confirmed FE-14 scope item from the Post-FE-13 Readiness Audit). A user with valid access — including a user with zero accessible projects — can now create a project entirely through the AIQA UI, using the real, already-existing backend endpoint (`POST /api/v1/projects`) and the real, already-existing, previously-unused frontend transport function (`aiqaApi.projects.create`). **Implemented: yes.**

## 3. Exit Criterion

GAP-01 (Project Creation UI, Type `FRONTEND_MISSING`, Priority P0) is closed when: a user can create a project through the real UI; the UI uses the real backend API via `aiqaApi.projects.create` as the runtime path; no backend code changes; no fake project on failure; pending state prevents duplicate creation; the new project enters real existing project state; a zero-project user is not blocked; API and UI tests cover the new behavior; existing tests remain passing; typecheck and build pass; FE-1 through FE-13 functionality is preserved; no out-of-scope gap was implemented.

**Satisfied: yes** — see §10–§18 for evidence of each clause.

## 4. Baseline

| Layer | State before FE-14 |
|---|---|
| Backend create API | REAL — `POST /api/v1/projects` (`services/orchestrator/src/api.rs::create_project`), Admin-role gated, validates `name` (trimmed, non-empty), `description` genuinely optional |
| Frontend transport | REAL — `aiqaApi.projects.create(body, signal)` (`forentend/src/services/aiqaApi.ts:349-350`), typed, zero runtime consumers, zero test coverage |
| Runtime UI consumer | **MISSING** — no component called it anywhere |

Re-confirmed by direct inspection at the start of this unit (not assumed from the audit report): read `api.rs::create_project`/`validate_project_name`/`CreateProjectBody` directly, and grepped for `aiqaApi.projects.create` in `forentend/src` — zero matches before this unit's changes.

## 5. Repository Inspection

Files/components actually read before writing any code:
- `forentend/src/services/aiqaApi.ts` (full `projects`/`executions`/`evidence`/`findings`/`dashboard`/`audit` transport, `ApiError`/`ApiErrorKind`, `requestJson`/`rawRequest`)
- `forentend/src/services/aiqaApi.test.ts` (existing test style/structure, to place new tests consistently)
- `forentend/src/context/ProjectContext.tsx` (full file — `applyList`, `selectProject`, `refreshProjects`, persisted-selection mechanism)
- `forentend/src/components/project/ProjectSelector.tsx` and `ProjectSelector.test.tsx` (full files — the only existing project-selection UI and its real test patterns)
- `forentend/src/components/layout/Header.tsx` (confirmed `ProjectSelector` placement, confirmed no dedicated `Modal`/`Dialog` component exists anywhere in the codebase — `grep -R "Modal|Dialog"` found only unrelated matches: a stale doc comment and `AIQAContext`'s dead `RunTriggerModal` reference)
- `forentend/src/types/contract.ts` (`CreateProjectRequest`, `Project`)
- `services/orchestrator/src/api.rs` (`create_project` handler, `validate_project_name`, `CreateProjectBody` struct, the router's `.route("/api/v1/projects", post(create_project)...)` line, and `require_role(&ctx, Role::Admin)` — confirmed project creation is **Admin-only** server-side)
- `forentend/src/context/AuthContext.tsx` (confirmed the frontend does **not** track the current key's role at all — role-gating cannot be pre-empted client-side, only handled truthfully on 403)

No filenames or component APIs were assumed — every one above was located by search/read before use.

## 6. Implementation

- **UI entry point**: a "Create Project" row inside the existing header `ProjectSelector` dropdown (`forentend/src/components/project/ProjectSelector.tsx`), shown whenever `auth.status === 'authenticated'`, regardless of whether `projects` is empty. No new page, no new Modal component (none existed in the codebase — introducing one would have been a design-language expansion beyond scope).
- **Form**: inline within the same dropdown panel (reuses its existing dark-panel visual language). Fields: `Name` (required) and `Description` (optional) — exactly the real `CreateProjectRequest` shape, no invented fields.
- **Validation**: client-side check mirrors the real backend rule exactly (`validate_project_name`: trimmed, non-empty) — checked only to avoid a request that would deterministically fail, not a stricter invented rule.
- **API integration**: `ProjectContext.createProject(body)` → `aiqaApi.projects.create(body)` → real `POST /api/v1/projects`.
- **Pending/loading**: `createSubmitting` state disables both the submit and cancel buttons, shows a spinner + "Creating…" label, and guards `handleCreateSubmit` against a second concurrent call.
- **Success**: the real returned `Project` is appended to `ProjectContext.projects` and selected immediately; the form and dropdown close.
- **Error**: the real `ApiError` (validation, forbidden, offline, server_error, etc.) is caught and its `.message` shown inline in the still-open form; nothing is created locally.
- **Project state update**: handled entirely inside `ProjectContext` (see §8).

## 7. API Reuse

**Confirmed: `aiqaApi.projects.create` was reused as-is, unmodified.** No raw `fetch`, no second API client, no component-local wrapper, no manually-constructed `Authorization` header. `ProjectContext.createProject` (the one new method added) calls `aiqaApi.projects.create` directly and does nothing else transport-related — it is exactly the same abstraction level `refreshProjects`/`fetchProjects` already use for `aiqaApi.projects.list`.

## 8. Project State Integration

`ProjectContext.createProject` (`forentend/src/context/ProjectContext.tsx`):
```ts
const project = await aiqaApi.projects.create(body);
setProjects((current) => [...current, project]);
setSelectedProjectIdState(project.id);
persistSelection(project.id);
return project;
```
The backend's own POST response body **is** the authoritative new `Project` record — it is appended directly to the existing `projects` array (no second state store, no re-fetch required) and immediately selected via the same `selectedProjectId`/`persistSelection` mechanism `selectProject` already uses. No `location.reload()` anywhere; live-verified (§15) that the header, sidebar, and Dashboard all reflect the new project without a reload.

## 9. Zero-Project Flow

Live-verified with two real zero-accessible-projects API keys (one Admin-role, one Operator-role, each minted with a `project_id` scope pointing at a nonexistent project so `GET /api/v1/projects` genuinely returns `[]`):
- The "Create Project" entry point is present in the "No accessible projects" panel — no pre-existing selection is required to reach it.
- The Admin-scoped zero-project key successfully created its first project through the real UI (`POST /api/v1/projects` → 201 → real project `26193954-e907-496d-ad17-aab00e61692d`, "First Project For A New Admin"), which became active immediately.
- The Operator-scoped zero-project key hit a real 403 (`"role 'operator' cannot perform this action (requires 'admin')"`) — the entry point stayed reachable and the error was shown truthfully; no project was created.

## 10. Backend Changes

**Expected: NONE. Confirmed: NONE.** `git diff --stat -- services/orchestrator/` returned no output before and after this unit. `services/orchestrator/` was never opened for editing, only read (§5).

## 11. Fake Data Verification

- **No fake project**: every project shown in the UI after this unit's changes came from a real `201 Created` backend response (verified independently against `orchestrator.sqlite3`'s `projects` table for both live-created projects — see §15).
- **No fake project ID**: IDs are the real `Uuid::new_v4()` values `api.rs::create_project` generates server-side; never client-generated.
- **No local success fallback**: `createProject` has no catch-and-succeed-anyway path; any thrown `ApiError` propagates to the caller and is surfaced as an error, never swallowed into a fabricated success.
- **No simulated creation request**: no `setTimeout`/`Math.random`/artificial delay anywhere in the new code (confirmed by grep — see §16).
- **No false success after failure**: the 403 scenario (§9, §15) was live-exercised and produced a real error state, zero new database row (confirmed via direct SQLite query — see §15).

## 12. Tests

| | Count |
|---|---|
| Before FE-14 | 261 passing (14 files) |
| After FE-14 | **272 passing (14 files)** |
| New tests added | 11 |

New tests:
- `forentend/src/services/aiqaApi.test.ts` — 5 new tests under `describe('aiqaApi.projects.create (FE-14 §16 ...)')`: real path/method/body + typed response; description genuinely optional (omitted, not sent as `""`); 403 → `forbidden`; 400 → `validation_error`; network failure → `offline`.
- `forentend/src/components/project/ProjectSelector.test.tsx` — 6 new tests under `describe('ProjectSelector — FE-14 real project creation (GAP-01)')`: entry point reachable (existing-project user); entry point reachable (zero-project user, brief §15); empty-name rejected with no request sent; successful creation enters state and becomes selected without a reload; real 403 failure shows a truthful error and creates nothing; duplicate submission prevented while pending.

Exact command/result:
```
npx vitest run
Test Files  14 passed (14)
     Tests  272 passed (272)
```

## 13. Typecheck

```
npx tsc --noEmit
```
Exit 0, no output — clean.

## 14. Build

```
npx vite build
```
```
✓ 2711 modules transformed.
dist/index.html                  0.77 kB
dist/assets/index-B9bE2HvY.css  89.15 kB
dist/assets/index-BfP4ZfGW.js 1,022.79 kB
✓ built in 23.39s
```
Succeeded (the pre-existing >500kB chunk-size advisory is unrelated to this unit's ~2kB addition and was already present before FE-14).

## 15. Manual Verification

Real backend (`aiqa-orchestrator.exe`, `http://127.0.0.1:8080`) and real frontend (`http://localhost:3000`) both started; driven live in-browser. Every scenario below was genuinely exercised this session:

- **A — Existing-project user**: global Admin key (2 pre-existing real projects) → selector shows both real projects, "Create Project" entry point present.
- **B — Zero-project user**: Admin key scoped to a nonexistent project → "No accessible projects" shown, "Create Project" entry point still reachable (no dead end).
- **B (empty name)**: submitted the create form with no name → "Project name is required." shown inline, no network request fired, form stayed open.
- **C — Valid creation**: submitted `"FE-14 Manual Verification Project"` via the global Admin key → real `201`, header/sidebar updated to the new project immediately, no reload. Independently confirmed present in `orchestrator.sqlite3.projects` with the exact typed description.
- **D — New project activation**: after creation, `ProjectContext`'s selection was the new project; Dashboard/Execution Hub/etc. all scoped to it correctly afterward.
- **E — Empty required name**: see B (empty name) above.
- **F — Backend failure**: Operator-role zero-project key submitted a real name → genuine `403` from the real backend (`"role 'operator' cannot perform this action (requires 'admin')"`) shown verbatim; independently confirmed zero matching row was ever inserted into `projects`.
- **G — Duplicate submission**: covered by the automated test (§12) with a controlled pending-promise fetch mock — a second click while the first request is in flight fires no second `POST`. (Not additionally re-exercised by hand against the real backend, since the real backend responds well under human click-speed; the automated test is the rigorous check for this race.)
- **H — Project switch**: switched between "AIQA Core Platform" (pre-existing) and the newly created project via the header selector — correct project-scoped data loaded each time, no stale/leaked data observed.
- **I — Regression**: with a real pre-existing project selected, navigated to and visually confirmed real, unregressed content on **Dashboard** (real KPIs/task counts/donut chart), **Test Generator** (unchanged, "Ready to Generate" flow intact), **Execution Hub** (real run list), **Audit Trail** (real entries, including the real `project.create` audit rows this unit's own actions produced), and **Evidence Viewer** (real execution list).
- **Zero-project Admin success**: a second, distinct Admin key scoped to a nonexistent project successfully created its own first project (`"First Project For A New Admin"`) — proving §9's zero-project flow with a role that is actually permitted to create, not just the reachability of the entry point.
- **Independent backend-audit-log confirmation**: queried `orchestrator.sqlite3.audit_log` directly — both real UI-driven creations appear as genuine `project.create` / `outcome: ok` rows, attributed to the exact API keys used in-browser.

All backend/frontend processes were stopped after verification (`taskkill` on both PIDs).

## 16. Search Verification

```
grep -R "projects.create" forentend/src
```
→ exactly one real runtime call site (`ProjectContext.tsx:208`, inside `createProject`), plus the new test files and one doc-comment reference. No duplicate.

```
grep -R "CreateProject" forentend/src
```
→ `ProjectContext.tsx` (the `createProject` method + its `CreateProjectRequest` type import), `aiqaApi.ts` (pre-existing type/function, untouched), `types/contract.ts` (pre-existing type, untouched). No second creation abstraction.

```
grep -R "ProjectSelector" forentend/src
```
→ `ProjectSelector.tsx`/`.test.tsx` themselves and `Header.tsx` (its one pre-existing mount point, untouched). No duplicate selector.

- Raw `fetch(` inside `ProjectSelector.tsx`: **none**.
- `setTimeout`/`Math.random` inside the new code (`ProjectContext.tsx`, `ProjectSelector.tsx`): **none**.
- Fake project ID / fake fallback project: **none** — every project value in state traces to a real `aiqaApi.projects.list`/`.create` response.

**Confirmed**: `aiqaApi.projects.create` now has a legitimate runtime consumer; no duplicate API abstraction exists; no raw transport bypass was added; no fake project fallback exists.

## 17. Security

- Authenticated transport reused unmodified — `createProject` goes through `aiqaApi.projects.create` → `rawRequest`, the same `Authorization: Bearer <key>` injection every other real call already uses.
- No API key exposed in the UI, logs, or URLs (the create request body is `{name, description?}` only).
- No hardcoded bearer token added anywhere.
- No credential placed in a URL/query string.
- Backend remains the sole authorizer — the Admin-role check lives entirely in `api.rs::create_project` and was not touched; the frontend does not attempt to pre-authorize or bypass it, and genuinely surfaces the backend's own 403 when a non-admin key is used (live-verified, §15 scenario F).

## 18. Regression

Actual checks performed (not assumed):
- Full automated suite: 272/272 passing, including all pre-existing FE-1–FE-13 tests unchanged in behavior (`ProjectSelector (FE-5 §20 items 17-20)`'s original 6 tests all still pass verbatim).
- `npx tsc --noEmit`: clean.
- `npx vite build`: succeeds.
- Live manual regression pass (§15, scenario I) across Dashboard, Test Generator, Execution Hub, Audit Trail, Evidence Viewer — all rendered real, correct, unregressed content with a real project selected.
- Project selection/switching (§15 scenario H) and project scope isolation (no stale cross-project data observed when switching) both verified live.

## 19. Files Changed

| File | Why |
|---|---|
| `forentend/src/context/ProjectContext.tsx` | Added `createProject(body)` to `ProjectContextValue` and the provider — the one new piece of real state-management logic, reusing `aiqaApi.projects.create` and the existing `projects`/`selectedProjectId`/`persistSelection` mechanism. |
| `forentend/src/components/project/ProjectSelector.tsx` | Added the "Create Project" entry point and inline creation form (name/description fields, validation, pending state, error display) inside the existing dropdown panel. |
| `forentend/src/services/aiqaApi.test.ts` | Added 5 tests giving `aiqaApi.projects.create` its first real test coverage (success, optional-description omission, 403, 400, offline). |
| `forentend/src/components/project/ProjectSelector.test.tsx` | Added 6 tests covering the new UI's minimum required scenarios (A–E from the brief, plus the zero-project variant of A). |

No other file was touched. `services/orchestrator/` was not modified. No product/contract document was modified.

## 20. Limitations

- Scenario G (duplicate submission) was verified via the automated test's controlled pending-promise mock, not by attempting to out-race the real backend by hand — a deliberate, honest choice (see §15) rather than a claim of something not actually done.
- A zero-project Admin key's `list_projects` call is itself scoped to that key's own `project_id` (a real, pre-existing backend behavior, not something FE-14 introduced) — so a project created by such a key is visible to a *different*, more broadly-scoped key (as shown in §15), but not necessarily to that same narrowly-scoped key on a subsequent list call unless its scope happens to match. This is existing backend project-scope behavior, unrelated to and unaffected by FE-14's correctness.
- `aiqaApi.projects.update` remains unused/untested — genuinely out of scope for FE-14 (GAP-01 is creation only) and was not touched.

## 21. Out of Scope

Confirmed: all non-GAP-01 work remained untouched this unit. Specifically not implemented:
- Model Gateway / real LLM integration for Test Generator.
- Any change to Test Generator's generation behavior or disclosure state.
- Realtime event-vocabulary changes or new WebSocket/subscription logic.
- Dashboard KPI expansion or redesign.
- Reports PDF/Excel/per-run/audit/test/bug-report generation.
- Execution DAG/dependency editor UI.
- Any of the 7 unsupported modules (AIQA Orchestrator, Test Case Validator, RBAC Testing, API Testing, Mobile Testing, Custom Testing, Feature Map/Flow).
- Project editing, deletion, archival, or new project permissions/roles.
- Any backend change of any kind.

## 22. Remaining Gaps

All gaps identified in `AIQA_POST_FE13_FE14_READINESS_CAPABILITY_GAP_AUDIT_REPORT.md` **other than GAP-01 remain unchanged** by this unit. In particular, still open exactly as that audit described them: GAP-02 (Test Generator disclosure/backend decision), GAP-03 (`INFRA_ERROR` contract naming), GAP-04 (Acceptance Matrix staleness), GAP-05 (realtime vocabulary decision), GAP-06 (dependency editor UI), GAP-07 (Dashboard KPI expansion), GAP-08/09/10 (report-generation family), GAP-11 (7 unsupported modules), GAP-12 (Recovery/Fault Tolerance verification). None of these was resolved, reduced, or otherwise affected by implementing GAP-01 — this unit does not claim any broader readiness improvement than the single capability it closed.

## 23. Final Result

**FE-14 closed GAP-01 — Project Creation UI.**

All Final Exit Criteria are satisfied:
- ✅ A user can create a project through the real UI.
- ✅ The UI uses the real existing backend API (`POST /api/v1/projects`).
- ✅ `aiqaApi.projects.create` is the runtime API path (sole consumer, confirmed by search).
- ✅ No backend code was changed.
- ✅ No fake project is created on failure (live-verified with a genuine 403).
- ✅ Pending state prevents duplicate creation (automated test + real disabled-button behavior).
- ✅ The new project enters the real existing `ProjectContext` state.
- ✅ A zero-project user is not blocked from creating their first project (live-verified with two distinct zero-project keys, one succeeding as Admin, one truthfully rejected as Operator).
- ✅ API and UI tests cover the new behavior (11 new tests).
- ✅ Existing tests remain passing (272/272, up from 261/261, zero regressions).
- ✅ Typecheck passes.
- ✅ Build passes.
- ✅ FE-1 through FE-13 functionality is preserved (live regression pass across Dashboard/Test Generator/Execution Hub/Audit/Evidence).
- ✅ No out-of-scope gap was implemented (§21).
