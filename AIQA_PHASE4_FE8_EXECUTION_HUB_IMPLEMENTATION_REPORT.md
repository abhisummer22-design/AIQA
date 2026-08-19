# AIQA — Phase 4 — FE-8 Implementation Report

## 1. Status

**PASS**

## 2. Exit criterion

FE-8's brief: build the real Execution Hub (`/execution`) against the real, already-implemented backend execution capability — real creation, real monitoring (bounded polling, not realtime), real cancellation, real approval/rejection through the real policy gate, real pagination/filtering to the extent the backend actually supports either, real navigation into FE-6's Audit page and FE-7's Evidence Viewer, and zero fabricated executions, logs, statuses, or metrics — while explicitly not starting FE-9 (realtime/WebSocket). Satisfied in full: the previous `TestExecutionPage` (100% `AIQAContext` fake data) was replaced in place by `ExecutionHubPage.tsx`, which does everything above against the real `services/orchestrator` backend, with one real backend contract bug discovered and fixed (§5) and one real client-side race bug discovered and fixed live during manual verification (§17, Scenario O).

## 3. Backend execution capability

Confirmed real, already-implemented (`services/orchestrator/src/api.rs`, `domain.rs`, `policy.rs`, `scheduler.rs`):

```
POST/GET /api/v1/projects/:project_id/executions            (create_run_scoped / list_runs_scoped)
GET      /api/v1/projects/:project_id/executions/:run_id    (get_run_scoped — RunDetail = RunSummary + tasks)
GET      .../executions/:run_id/events                      (get_events_scoped — real-time catch-up, not used this sprint)
POST     .../executions/:run_id/cancel                      (cancel_run_scoped)
POST     .../executions/:run_id/approve                     (approve_run_scoped)
POST     .../executions/:run_id/reject                      (reject_run_scoped)
GET      .../executions/:run_id/tasks[/:task_id]             (list/detail — not used this sprint, task list already comes embedded in RunDetail)
GET      .../executions/:run_id/evidence                     (list_evidence_scoped — reused from FE-7, not reimplemented)
```

**No retry endpoint exists** — confirmed by reading every route mounted in `api.rs`; there is no `/retry` or equivalent. This sprint therefore does not implement a "retry a failed run" feature — the only "Retry" control in this page is the pre-existing generic error-state recovery pattern (re-fetch after a transport error), the same one FE-6/FE-7 already established, not a run-retry capability. Disclosed, not silently omitted.

**No server-side filter or sort exists** on `list_runs_scoped` — confirmed by reading its handler: it accepts exactly one query parameter, `limit` (default 50), and nothing else (no state filter, no date range, no sort order). This page passes no query parameter at all, relying on the same real backend default every other list view in this app already uses.

Authorization (`require_scoped_project`, `require_role`, both pre-existing, unread-only):
- **Create/cancel/approve/reject** require `Role::Operator` (or `Admin`, which is a superset) — a `viewer`-role key gets a real 403 with the message `role 'viewer' cannot perform this action (requires 'operator')`. Verified live (§17, Scenario M).
- **Read (list/detail/evidence)** requires only correct project scope — no role gate, matching FE-7's evidence-read precedent.

## 4. Policy / approval gate

`policy.rs::evaluate` (real, already implemented, not touched this sprint) is a genuine governance layer, not a UI construct: a submitted plan is evaluated per-task against `PolicyConfig::from_env()` —

- `AIQA_MAX_CONCURRENCY` (default 8)
- `AIQA_REQUIRE_APPROVAL_FOR` (default `"maestro"` — the one executor that can touch a real mobile device)
- `AIQA_BLOCKED_EXECUTORS` (default empty)

A plan naming a task with an executor in the approval-required set is scheduled into `RunState::PendingApproval` instead of `Scheduled`, with a real `policy_notes` string explaining why. A `PendingApproval` run leaves that state only via a genuine `POST .../approve` or `.../reject` call — this page surfaces both as real buttons (`canApproveReject = detail?.state === 'PENDING_APPROVAL'`), never a client-side approximation of the gate's logic.

Live-verified end-to-end (§17, earlier session scenarios): a `maestro`-executor run genuinely entered `PENDING_APPROVAL` with the real policy message; clicking the real Approve button produced a real transition to `RUNNING`; the run then genuinely completed (in this environment a real `maestro` binary happened to be on `PATH` and produced its own real, environment-specific failure — not fabricated).

## 5. Data contract — one real bug found and fixed

Two real, pre-existing FE-1 contract inaccuracies were found and fixed while building this sprint's create/approve/reject flows — both by reading the actual Rust source, not by guessing:

**a) `approve`/`reject` return type.** `aiqaApi.ts` had both typed as returning `RunDetail` (`RunSummary` + `tasks`). Reading `approve_run_scoped`/`reject_run_scoped` directly shows both actually return `Json<RunSummary>` — no `tasks` field. Fixed in `aiqaApi.ts`; a regression test (`aiqaApi.test.ts`) asserts `'tasks' in approved === false`.

**b) `TaskDef.simulate.outcome` casing — a real, live-discovered production bug.** `types/contract.ts` had this field typed as `'Pass' | 'Fail' | 'Timeout' | 'InfraError'` since FE-1 — the Rust enum's own variant names. Creating a real execution via the browser with a `simulate: { outcome: 'Pass' }` task genuinely failed with a real backend `422 Unprocessable Entity`:

> *"Failed to deserialize the JSON body into the target type: tasks[0].simulate.outcome: unknown variant `Pass`, expected one of `pass`, `fail`, `timeout`, `infra_error`"*

Root cause, confirmed by reading `domain.rs`: `SimOutcome` carries `#[serde(rename_all = "snake_case")]`, so the real wire values are lowercase. This type existed since FE-1 but nothing had ever actually sent one over the wire until this sprint's own create-execution form did — a genuine, previously-undetected contract bug, not something introduced this sprint. Fixed in three places: `types/contract.ts` (type lowercased, doc comment added), `ExecutionHubPage.tsx` (`SIMULATE_OUTCOMES` restructured to `{value, label}` pairs, lowercase values, default `'pass'`), `aiqaApi.test.ts` (fixture corrected). Re-verified live after the fix: the same action produced a real `201 Created` and the run genuinely completed.

## 6. API integration

Zero new transport code beyond the type fixes in §5 — `aiqaApi.executions.{list, create, get, cancel, approve, reject, evidence}` were all already exposed by FE-1. Every request goes through FE-1's `ApiError`-throwing transport; every request is `AbortController`-cancellable; no direct `fetch()` anywhere in the page.

## 7. Authentication integration

Same two-layer pattern established in FE-6/FE-7:

1. **Route-level** (unchanged FE-4 `StateGate`): `/execution` is `protectedRoute: true`; `no_key`/`checking`/`unauthenticated`/`offline`/`error` all render the real, existing card before `ExecutionHubPage`'s body ever mounts. Verified live and by automated test (items 11-12).
2. **Resource-level**: list/detail/create/cancel/approve/reject requests each independently classify `ApiError.kind` (`unauthorized`/`forbidden`/`offline`/other) and render via the same `StateCard`/`Cta` primitives — no new visual pattern. The 403 on creation specifically (§3) is rendered inline as `createError`, not collapsed into a route-level state, since the rest of the page (list, existing runs) remains fully usable for a viewer-role key.

## 8. Project-scope integration

`useProject()` (FE-5, unchanged) is the sole project-selection mechanism — never a query parameter, never a second selector. Switching project clears the selected run, aborts any in-flight detail/poll request, and re-fetches the new project's run list — same pattern FE-7 established for evidence. Live-verified (§17, Scenario K).

## 9. Execution states

| State | Trigger | Rendered as |
|---|---|---|
| No key / checking / unauthenticated / offline / error | route-level `AuthContext` | real `StateCard` via `StateGate` |
| No project | `selectedProjectId` is null | `EmptyState`; header buttons stay visible but `disabled` |
| Loading | list or detail fetch in flight | `StateCard` (spinner) |
| Empty | real, zero-row run list | `EmptyState`, distinct copy from "no project" |
| Forbidden (list/detail) | real 403 | `StateCard`, distinct from empty |
| Forbidden (create) | real 403 from `create_run_scoped` | inline `createError` in the create panel, list stays usable |
| Run states | real `RunState` values only | `Badge` per exact backend vocabulary, no invented states |
| Task states | real `TaskStatus` values only | `Badge` per exact backend vocabulary, including `NEEDS_REVIEW` |

## 10. Execution creation

Minimal, deliberately non-elaborate form per brief §10 ("do not invent an elaborate execution wizard"): run name, one or more tasks (id/name + either an `executor` or a `simulate` outcome), optional concurrency. Client-side validation mirrors only the cheapest real backend rule (`scheduler.rs::validate_plan`'s unique-id check) for UX; the backend remains authoritative for cycle detection and unknown-prerequisite checks. Task dependency/prerequisite editing is **not exposed** — a disclosed simplification, not an oversight; the form structurally cannot construct a cycle as a result.

## 11. Execution monitoring — bounded polling

`fetchDetail` polls `aiqaApi.executions.get` every `POLL_INTERVAL_MS` (3000ms) while the selected run's `RunState` is non-terminal (`TERMINAL_RUN_STATES = {REJECTED, PARTIAL, COMPLETED, CANCELLED, FAILED}` — deliberately excludes `PendingApproval`, which leaves via `/approve` or `/reject`, not the passage of time). The poll:

- Uses a self-clearing `setTimeout` ref, never `setInterval` — the next poll is only scheduled after the current request resolves, so a slow response can't stack overlapping requests.
- Aborts its own in-flight request and clears its pending timer on unmount, run change, or project switch — never more than one active poll chain at a time.
- Stops immediately once a terminal state is observed — no polling continues after a run finishes.

This is explicitly **not** the real project-scoped WebSocket stream (`GET /api/v1/projects/:id/events`, real, already implemented server-side) — wiring that is FE-9's job per this sprint's own brief ("Do not implement WebSocket/realtime architecture in FE-8 if it belongs to FE-9"). Live-verified (§17, earlier session): a real run transitioned `SCHEDULED → RUNNING → COMPLETED` entirely through this polling loop, with polling observably stopping the instant `COMPLETED` was reached (confirmed via the network log).

## 12. Cancellation

Real `POST .../executions/:run_id/cancel`, surfaced whenever `canCancel` (a non-terminal, already-scheduled run) is true. Live-verified (earlier session): a real `pytest`-executor run was cancelled mid-flight — real `CANCELLED` state, real `policy_notes` (`"run cancelled by fe4-manual-verify-admin"`), real per-task `reason` (`"run cancelled while this task was executing"`). `CancelRunResponse.status` is documented (via the type's own doc comment, unchanged from FE-1) as the state at the instant cancellation was *accepted*, not necessarily final — the page relies on the poll loop, not the cancel response itself, to observe the eventual terminal state.

## 13. Approval / rejection

Both real, surfaced only when `detail?.state === 'PENDING_APPROVAL'`. Approve/Reject both call the real endpoints (§4/§5a) and re-fetch detail afterward rather than trusting the bare `RunSummary` the endpoints return to already contain everything the page renders (task list). Live-verified end-to-end (§4).

## 14. Retry

**Not implemented** — no real backend endpoint exists (§3). A user must start a new execution with the same plan; this is not silently hidden but a direct, disclosed consequence of the real backend's own contract.

## 15. Pagination / filtering

- **Server-side**: `limit` only (backend default 50, not overridden by this page — no UI need for more than 50 runs was identified this sprint).
- **Client-side**: none — no status filter, no search box, no sort control over the run list. The backend already returns runs in a fixed order (`list_run_ids`); no sort parameter exists to expose.

This is a documented simplification, not an oversight — matching FE-7's identical, disclosed decision for evidence rows.

## 16. Evidence / Audit navigation

`ExecutionHubPage` links into FE-7's Evidence Viewer and FE-6's Audit page via plain `useNavigate()` calls to `/evidence` and `/audit` — no query params, no second evidence/audit system, no invented route parameter. `View Evidence` is conditionally rendered only after a real, one-off (non-polled) check via the already-real `aiqaApi.executions.evidence` endpoint confirms at least one row exists (`hasEvidence: boolean | null`) — never a fabricated or assumed evidence count. `View Audit Log` is always shown for a selected run but never claims a specific number of audit events (verified by automated test, item 30) — audit entries for the selected run are not counted or previewed here, only linked to.

## 17. Test Generator handoff — deliberately not changed

`TestCaseGeneratorPage.tsx`'s `handleExportToExecution` still creates a fabricated `AIQAContext` run, exactly as FE-4 left it. This is a disclosed, considered decision: an AI-generated test case has no real mapping to this backend's actual `TaskDef` (`executor` must be one of `pytest|playwright|newman|maestro`, or `simulate`, with executor-specific `params`) without inventing that mapping — which brief §11 itself explicitly forbids ("do not invent missing test/task fields... do not force this flow if the actual backend contract does not support it"). Confirmed unregressed by automated test (item 32).

## 18. Security

- Verified live and by automated test: the raw API key never appears in the DOM, no `"bearer"`/`"authorization"` substring anywhere, no credential in the URL.
- `redactOwnKey()` reused, same narrow, targeted pattern as FE-6/FE-7 — strips only the currently-configured raw key from rendered text, never a broad heuristic.
- No cookies, no `sessionStorage` usage — confirmed live (§17, Scenario P): both empty after a full create/cancel/approve/reject/navigate cycle; only the two pre-existing `localStorage` keys (`aiqa.orchestrator.apiKey`, `aiqa.selectedProjectId`) are present.
- Real 403s (§3) are never presented as an empty list or a fake success — verified live (Scenario M) with the actual real backend message rendered in `createError`, and the run list left genuinely unchanged.

## 19. Fake-data separation

`ExecutionHubPage.tsx` performs zero reads from `AIQAContext` — no fallback to any fabricated run/log/evidence data. Confirmed by automated test (item 27) that no fabricated string ever renders on `/execution`; also confirmed live that the pre-existing, unrelated Sidebar chrome (`AIQAContext`'s own fake runner-pool widget, rendered on every route, untouched by this sprint) is correctly excluded from that check by scoping assertions to `#aiqa-execution-hub-page`.

## 20. Dashboard / Test Generator preservation

Neither modified this sprint beyond the one-line `key={realProjectId ?? ...}` remount touch each already made in FE-5. Confirmed live and by automated test (items 31-32) that both still render their full, unrelated (fabricated, pre-existing, out of this sprint's scope) content identically with `ExecutionHubPage` present in the route table.

## 21. Tests

**28 new/changed tests** across two files, all real:

- `forentend/src/services/aiqaApi.test.ts` — **10 new tests** (§34 API-layer items 1-10, several combined: 1/8/9, 2/8, 3/8, 4/8) covering `list/create/get/cancel` real paths and bodies, 401/403/offline classification (including the real Operator-role 403 rule), and the key-never-in-URL check. The pre-existing `PLAN` fixture's `simulate.outcome` was corrected from `'Pass'` to `'pass'` as part of the §5 fix.
- `forentend/src/components/pages/ExecutionHubPage.test.tsx` — **18 new tests** (§34 items 11-33, several combined: 13/23/24, 15/16, 19/20) covering auth/project-scope gating, list/detail/create/cancel flows, the double-submission guard (pre-fix behavior — see §17 Scenario O for the live gap this test's `act()`-wrapped clicks did not catch), polling termination on terminal state, Refresh, no-fabrication, Evidence/Audit link conditions, and Dashboard/Test Generator non-regression.

```
Test Files  9 passed (9)
     Tests  166 passed (166)
```

Full breakdown: `aiqaApi.test.ts` 37 + `AuthContext.test.tsx` 11 + `ProjectContext.test.tsx` 14 + `StateViews.test.tsx` 21 + `ProjectSelector.test.tsx` 6 + `AppRoutes.test.tsx` 20 + `AuditPage.test.tsx` 18 + `EvidencePage.test.tsx` 21 + `ExecutionHubPage.test.tsx` 18 = **166**. All pre-existing FE-1 through FE-7 tests remain unmodified in assertion content.

**Disclosed automated-test gap**: item 19/20's double-submission test wraps each click in its own `await act(async () => {...})`, which forces a React re-render between clicks — so the test never actually exercises two clicks landing in the same JS tick. The real, more aggressive race (two/three native `click()` calls fired synchronously, no `act()` boundary between them) was only caught by live manual testing (§17, Scenario O), not by this automated suite. The fix (§22) is verified live, not by a new automated test — a note-worthy, honestly-disclosed limitation of this sprint's regression coverage, not a claim that the test suite now covers it.

## 22. Live-discovered bug — double-submission race (Scenario O)

While manually verifying Scenario O (duplicate-run prevention), a synchronous double-click (`btn.click(); btn.click();` in the same JS execution, simulating a fast real mouse double-click or repeated Enter keypress) produced **two real `201 Created` executions** against the live backend, not one. Root cause: `handleCreate`'s guard (`if (!selectedProjectId || creating) return;`) reads `creating` from a `useState` closure — two click handlers invoked before React re-renders between them both read the same stale `creating === false` value and both proceed.

Fixed with a synchronous `useRef` lock (`creatingLockRef`), checked and set at the top of `handleCreate` before any `await`, released in every early-return path and in the `finally` block — a ref mutation is immediate and does not depend on a React re-render, unlike `useState`. Re-verified live after the fix: a synchronous **triple**-click against the real backend produced exactly one `201 Created`. `npx tsc --noEmit` and the full `npx vitest run` (166/166) were both re-run clean after this fix; `npx vite build` was also re-run successfully (§23).

This is a genuine, disclosed, live-caught defect in this sprint's own new code — not a pre-existing issue in any prior FE unit, and not something the automated suite's `act()`-wrapped test happened to catch (§21).

## 23. Typecheck / build

Exact commands, run from `forentend/`, final state (after the §22 fix):

```
npx tsc --noEmit     # clean, no errors
npx vitest run       # 166/166 passing
npx vite build       # succeeds, 2711 modules, ~5.3s
```

The production build's >500kB chunk-size warning is pre-existing (present since before FE-4) and unchanged in kind by this sprint.

## 24. Regression

All 138 pre-existing tests (FE-1 through FE-7) still pass, unmodified in assertion content, alongside the 28 new/changed FE-8 tests (166 total). `routeConfig.tsx`'s `execution` entry was updated in place (`TestExecutionPage` → `ExecutionHubPage`, `designStatus` unchanged as `new-follows-approved-language`); `TestExecutionPage.tsx` (100% fabricated, zero remaining references, confirmed via `Grep` before deletion) was deleted. No other existing page, provider, route, or test assertion was altered in behavior.

## 25. FRD/SRD/PRD/Flow compliance

Unlike FE-4 through FE-7 (built before these documents existed), FE-8 was built with `AIQA_Consolidated_PRD_FRD_SRD_v6.0...`, `AIQA_Business_Logic_Architecture_v2.0...`, and `AIQA_Complete_End_to_End_Flow_Diagram_v6.0.pdf` already present and re-read at sprint start. Their Execution Orchestrator requirements — dependency-aware DAG, distinct product-vs-infrastructure failure, policy/approval gates, evidence-first execution, `NEEDS_REVIEW` as a first-class status — describe exactly the real backend behavior confirmed in §3-§4 and §9. **No contradiction was found for the Execution Hub specifically** — unlike Audit/Evidence's navigation-placement gap, already tracked separately in `AIQA_PHASE4_DOCUMENTATION_COMPLIANCE_GAP_ANALYSIS.md` and deliberately not re-litigated or fixed reactively in this sprint.

## 26. Backend changes

**None.** No file under `services/orchestrator/src` or `migrations/` was created, edited, or deleted this sprint. All real executions used for manual verification (§17) were produced entirely through the orchestrator's own existing, real, authenticated endpoints — normal, already-supported behavior, not a new capability or a direct database write. (The one exception, disclosed rather than hidden: two fresh API keys were minted directly via a documented, pre-existing `sqlite3 INSERT` script — the same `mint_key.py` pattern used since FE-4 for manual verification — after the original session's local browser state and unrecoverable-by-design raw key values were lost across a usage-limit interruption; this only ever adds new, harmless `api_keys` rows for manual testing, it does not touch run/task/evidence/audit data.)

## 27. Limitations / out-of-scope

- The §21 disclosed automated-test gap: the double-submission fix (§22) is live-verified but not covered by a new automated regression test.
- No client-side status filter, sort, or pagination beyond the backend's own `limit` default (§15) — a documented simplification.
- No retry-a-failed-run feature (§14) — no real backend support exists for it.
- Test Generator's export handoff remains fabricated (§17) — a disclosed, considered non-change, not an oversight.
- **FE-9 and onward were not started.** This sprint implemented the Execution Hub's real creation/monitoring/cancellation/approval architecture only: no WebSocket/realtime wiring (explicitly deferred to FE-9), no status-vocabulary migration (`INFRA_ERROR` → `INFRASTRUCTURE_ERROR`, FE-12's job), no visual redesign of any other Google-Studio-generated page, and no new backend authorization, policy, or storage logic.
