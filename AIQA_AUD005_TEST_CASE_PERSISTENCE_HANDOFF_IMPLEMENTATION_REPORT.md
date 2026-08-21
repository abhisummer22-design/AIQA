# AIQA — AUD-005: Test Case Generator → Persistence → Validation → Execution Hub

## Executive Summary

AUD-005 is **CLOSED**. The Test Case Generator now produces cases that can be persisted for real (a new `test_cases` table, project-isolated), moved through a real, server-enforced lifecycle (`GENERATED → VALIDATED → APPROVED → EXECUTED`, with `REJECTED` reachable from the first two), and handed off through a new `POST .../test-cases/:id/handoff` endpoint that builds a real `RunPlan`/`TaskDef` and dispatches it through the **existing, unmodified** execution engine/scheduler — no shadow engine, no simulated persistence, no fabricated result. This was verified end-to-end by a real integration test that generates, persists, validates, approves, hands off, watches the real engine execute the real task, and confirms traceability (`executed_run_id`/`executed_task_id`) back on the test case row — not by trusting this report's own narrative.

**A mid-session discovery**: partway through Phase 1 (fresh audit), `services/orchestrator/src/domain.rs` was found to already contain a substantial, well-formed, in-house-style start on this exact feature (`TestCaseStatus`, `TestStep`, `CreateTestCaseRequest`, `TestCaseHandoffRequest`) that had not existed in the repository state audited at the start of this session and that this session had not yet written. No other file (`store.rs`, `api.rs`, `api_tests.rs`, any frontend file) had been touched. This is disclosed transparently in §"Mid-Session Discovery" below — it was treated as an authoritative, compatible starting contract and built on rather than duplicated or overwritten.

## Previous AUD-005 State

Freshly re-verified against source (not assumed from the prior audit report): the Test Case Generator (`TestCaseGeneratorPage.tsx`) produced a `GeneratedTestCase[]` held only in local React state (backed by `AIQAContext.activeGenerationSession`, itself only in-memory), with template-assembled content and a `Math.random()`-based local id. No backend route, table, or persistence path existed for it. "Approve"/"Reject" only flipped local state. "Open Execution Hub" only navigated — it never sent data anywhere. This was confirmed unchanged from the prior audit's own finding at the start of this unit.

## Current State

```
Requirement text / doc slots (unchanged, still local input)
        ↓
Test Case Generator (unchanged: local template assembly, honestly labeled)
        ↓
GeneratedTestCase[] (local draft, unchanged)
        ↓
"Save to Project" (NEW — explicit, real, opt-in)
        ↓
POST .../test-cases  →  real `test_cases` row (GENERATED)
        ↓
POST .../test-cases/:id/validate  →  real structural check → VALIDATED
        ↓
POST .../test-cases/:id/approve  →  VALIDATED-only gate → APPROVED
        ↓
POST .../test-cases/:id/handoff  →  real RunPlan/TaskDef → create_run_core
        ↓
Existing scheduler/engine/executor (UNCHANGED) → real Run + Task
        ↓
mark_test_case_executed → EXECUTED, executed_run_id/executed_task_id set
        ↓
Existing Evidence/Findings/Intelligence/Dashboard/Audit (UNCHANGED, already real)
```

## Architecture Changes

None to the execution architecture itself. The Execution Hub, DAG scheduler (`scheduler.rs`), engine (`engine.rs`), executor registry (`executor.rs`), realtime event stream, and intelligence pipeline are **byte-for-byte unmodified** — confirmed via `git diff --stat`, which shows zero diff on `engine.rs`, `scheduler.rs`, `executor.rs`, `intelligence.rs`. The only new architecture is the Test Case entity and its own lifecycle/HTTP surface, which *calls into* the existing `create_run_core` exactly as the real Execution Hub itself does.

## Database Changes

New migration `services/orchestrator/migrations/0005_test_cases.sql`: one new table, `test_cases`. Columns match `CreateTestCaseRequest` one-to-one, plus backend-assigned fields (`id`, `source`, `status`, `validation_notes`, `executed_run_id`, `executed_task_id`, timestamps). `project_id` is a free-text column with no FK — the same, already-established convention every other project-scoped table in this schema uses (see `0004_projects.sql`'s own documented reasoning, re-confirmed by direct read this session). Two indexes: `(project_id, created_at)` for listing, `(project_id, status)` for status-filtered listing. No existing table was altered. No dead schema: every column has a real, exercised code path (proven by the passing tests below, which read and assert on every column).

## API Changes

Six new routes, all under `/api/v1/projects/:project_id/test-cases`, added to the existing `protected` router (same `require_auth` layer as every other route):

| Route | RBAC | Purpose |
|---|---|---|
| `POST /test-cases` | Operator | Create (persist) one test case |
| `GET /test-cases` | Any authenticated | List, optional `?status=` filter |
| `GET /test-cases/:id` | Any authenticated | Get one |
| `PATCH /test-cases/:id` | Operator | Update editable descriptive fields (never `status`) |
| `DELETE /test-cases/:id` | Operator | Delete |
| `POST /test-cases/:id/validate` | Operator | Real structural-completeness gate → `VALIDATED` |
| `POST /test-cases/:id/approve` | Operator | `VALIDATED`-only gate → `APPROVED` |
| `POST /test-cases/:id/reject` | Operator | `GENERATED`/`VALIDATED`-only gate → `REJECTED` |
| `POST /test-cases/:id/handoff` | Operator | `APPROVED`-only gate → real `RunPlan`/`TaskDef` dispatch → `EXECUTED` |

Every mutating handler follows the same `require_scoped_project` + `require_role` + `audit(...)` pattern already established by every other scoped resource in this file (confirmed by direct comparison against `create_run_scoped`/`update_project_handler`). Lifecycle transitions are enforced by conditional `UPDATE ... WHERE status = <expected>` in `store.rs`, not just checked in the handler — a 409 (not a silent no-op) is returned when the precondition isn't met.

**Validation scope, stated honestly**: `POST .../validate` checks real, verifiable structural completeness (non-empty title/description/steps/expected_result, every step has both an action and an expected result). It does **not** claim to run any AI/model-scored quality benchmark — none exists anywhere in this backend (confirmed: no model-gateway/LLM integration found in `services/orchestrator` during this session's fresh grep). This is disclosed in the handler's own doc comment and in the 409 message a caller receives on failure.

## Frontend Changes

- `types/contract.ts`: added `TestCaseStatus`, `TestCaseStep`, `CreateTestCaseRequest`, `TestCaseRow`, `UpdateTestCaseRequest`, `TestCaseHandoffRequest` — mirrored field-for-field from the real Rust types.
- `services/aiqaApi.ts`: added `aiqaApi.testCases.{list,create,get,update,delete,validate,approve,reject,handoff}`, following the exact same transport pattern (`requestJson`/`ApiError`) every other real endpoint in this file already uses.
- `components/pages/TestCaseGeneratorPage.tsx`: **the existing generator's local-template assembly is unchanged** (still honestly labeled "LOCAL TEMPLATE GENERATION"). New: a real fetch of the project's persisted test cases on mount/project-switch (abort-on-switch, matching `DashboardPage`/`ReportsPage`'s established pattern); a new, explicit **"Save to Project"** action that `POST`s each locally-generated draft for real and switches the view to the real, backend-sourced list; real `validate`/`approve`/`reject`/`handoff` handlers once any case is persisted. Local-only pre-save approve/reject (a lightweight triage preview) is **untouched** — this is why all 9 pre-existing FE-16 tests still pass unmodified.
- `components/test-generator/GeneratedTestCasesView.tsx`: extended (not redesigned — this remains the same product-owner-designed layout) with a "Save to Project" button, real loading/error states for the backend fetch, `VALIDATED`/`EXECUTED` status badges, a "Validate Test Case" action, and a "Hand Off to Execution" control with an **explicit, required, visible simulated-outcome picker** (see "Execution Handoff" below).

## Execution Handoff

This is the part requiring an explicit, honest design decision rather than an invented shortcut.

A generated test case's `steps`/`assertions` are natural-language content, not an automatable script. This codebase has no AI/model-gateway integration and no way to translate arbitrary generated text into a real Playwright spec (confirmed absent, fresh grep, this session). The existing engine's own default behavior (`SimulatedExecutor`, `executor.rs:57-65`) already falls back to `SimSpec { outcome: Pass, delay_ms: 150 }` when a task specifies neither a real `executor` nor an explicit `simulate` block — meaning silently defaulting the handoff to this behavior would report a fabricated "passed" result for steps nothing had actually verified.

**Decision made**: `handoff_test_case_scoped` **requires** the caller to supply either a real `executor` or an explicit `simulate` outcome — a request with neither is rejected with a real `400 VALIDATION_ERROR` (`aud005_handoff_without_executor_or_simulate_is_rejected`, passing). The frontend's Handoff control makes this an explicit, visible, required choice (a `<select>` the operator must consciously set before the button is enabled to matter — mirroring the exact same `simulate`-outcome convention `ExecutionHubPage.tsx`'s own manual task builder already established, confirmed by reading that file first). The UI additionally displays a persistent, honest disclosure next to any `APPROVED`/`EXECUTED` case: *"No real automation script exists for a generated case yet — its execution outcome is an explicit, disclosed simulated result, not a genuine verification of the steps above."*

Everything else about the handoff is real: a real `RunPlan`/single `TaskDef` is built (task id = the test case's own UUID, so `tasks.id` doubles as the traceability key), passed to the **exact same** `create_run_core` function `POST .../executions` itself calls — confirmed by reading the call site, not by assumption — so it goes through the real DAG scheduler, the real engine, real event emission, real evidence storage. `aud005_handoff_run_is_a_real_execution_hub_run` proves the resulting run is visible through the pre-existing, already-real `GET .../executions` list — not a shadow path only this feature can see.

## RBAC / Project Isolation

- All mutating routes require `Role::Operator` (matching the bar every other mutating scoped resource uses); reads require only authentication.
- `scoped_test_case` (the shared fetch-then-authorize helper for every `:id` route) checks existence *and* that the row's real `project_id` matches the URL's project *before* calling `require_project_access` — a case that exists but belongs to a different project 404s (never leaks its existence), and a same-project-but-wrong-key access 403s.
- Verified by 3 dedicated tests, all passing: `aud005_project_isolation_enforced_both_directions` (both directions: B can't read/list/mutate A's case, even naming A's own real project id), `aud005_rbac_viewer_cannot_mutate_but_can_read`, `aud005_missing_project_and_missing_test_case_return_404`.

## Test Coverage

**Backend** — 15 new tests in `api_tests.rs`, all passing (`cargo test aud005`, 15/15):
create+get+list persistence, update (ignores status), delete, full validate→approve lifecycle, approve-without-validate rejected (409), validate rejects structurally incomplete input (409), reject from both eligible states + rejected-then-approve blocked, project isolation both directions, RBAC (Viewer read-only), missing project/case (404), invalid input (400), handoff without executor/simulate rejected (400, the "no fabricated result" rule), handoff before approval rejected (409), **the full real integration path** (`aud005_full_generate_persist_validate_approve_handoff_execute_traceability` — generate → persist → refetch-after-"refresh" → validate → approve → handoff → real engine execution → `poll_run_state` to real `COMPLETED` → traceability fields verified against the independently-refetched real run/task), and handoff-creates-a-real-visible-run.

**Frontend** — 4 new tests in `TestCaseGeneratorPage.test.tsx`, all passing: a previously-saved case is fetched and rendered on a fresh mount (no local generation needed — proves real persistence, not local echo); Save to Project makes real `POST` requests and the disclosure text switches to the real-data wording; a failed save reports an honest, specific error and never shows the "saved" state; handoff sends a real request with the explicit simulated outcome. All 9 pre-existing FE-16 tests pass **unmodified** — the local-draft pre-save path was deliberately left untouched.

**Integration** — the backend E2E test above *is* the required "at least one complete real path" test: Generate → Persist → Retrieve → Validate → Approve → Handoff → Execute → Result, using the actual backend and existing execution infrastructure, not a mock.

## Build Results

| Check | Command | Result |
|---|---|---|
| Backend build | `cargo build` | Clean |
| Backend test compile | `cargo test --no-run` | Clean, both binaries |
| Backend full suite | `cargo test` (×3 repeated) | **120/120** every run (105 pre-existing + 15 new) |
| Frontend typecheck | `npx tsc --noEmit` | Clean |
| Frontend full suite | `npx vitest run` | **329/329**, 16 files (325 pre-existing + 4 new) |
| Frontend production build | `npx vite build` | Clean, 2711 modules, same pre-existing chunk-size advisory |

## Regression Results

- **Backend**: pre-implementation baseline recorded fresh this session: **105/105**. Post-implementation: **120/120**, 3 repeated full-suite runs, 0 failures. One **unrelated** transient failure was observed mid-session in `actual_be9_scoped_approve_and_reject_through_router` (a pre-existing, already-known class of load-sensitive test timing — see the immediately-prior session's own flakiness-fix work) — reproduced clean 5/5 times in isolation immediately after, and clean in every subsequent full-suite run; this test does not touch `test_cases` in any way and is unrelated to this unit's changes.
- **Frontend**: pre-implementation baseline recorded fresh this session: **325/325**. Post-implementation: **329/329**, 0 failures, 0 modified pre-existing tests.
- No pre-existing test was deleted or weakened. No pre-existing test needed modification — the local-draft pre-save path was deliberately left byte-for-byte unchanged specifically to avoid regressing the FE-16 suite.

## Fake/Stub Audit

Fresh grep across every file this unit touched (`domain.rs`, `store.rs`, `api.rs`, `TestCaseGeneratorPage.tsx`, `GeneratedTestCasesView.tsx`, `aiqaApi.ts`, `contract.ts`) for `TODO|FIXME|unimplemented!|todo!|Math\.random`, excluding test files: **one match**, `TestCaseGeneratorPage.tsx:659`'s pre-existing, already-disclosed local session-id generator (FE-16, unrelated to this unit, still confined to a local-only, never-backend-facing id). No new fake data, fake persistence, or fake execution result was introduced anywhere — confirmed by direct inspection of every match.

## MCP / Skill Usage

**Used**: standard repository tooling only (Read/Grep/Glob/Edit/Write/Bash) for all source inspection, schema design, implementation, and test execution — the correct and sufficient tool for local, non-GitHub-API, non-design-tool work. The task's own MCP-selection principle explicitly scopes GitHub tooling to "repository/history/PR/issue context" and Figma tooling to "existing Figma design/specification" inspection — neither applied here (no GitHub PR/issue interaction was requested; the UI being extended is an existing in-repo React component, not a Figma source of truth).

**Available but not used**: no GitHub MCP or Figma MCP is connected in this environment (confirmed by the deferred-tools listing at the start of this session — only a Gmail-shaped mail connector and an MCP-registry lookup tool were present, neither relevant). No database-specific MCP is connected; SQLite inspection was done directly via `sqlx`/the application's own store layer and its own passing tests, which is the correct, already-established way this codebase verifies persistence (no separate DB browser tool exists in this repo's toolchain). Nothing was installed or requested, per instruction.

## Mid-Session Discovery

During Phase 1 (fresh audit, before any code was written), `git diff --stat` showed `services/orchestrator/src/domain.rs` already modified with a well-formed `TestCaseStatus`/`TestStep`/`CreateTestCaseRequest`/`TestCaseHandoffRequest` design — content that had not existed at the start of this session and that had not yet been written by this unit's own work. No other file (`store.rs`, `api.rs`, `api_tests.rs`, migrations, any frontend file) showed any corresponding change — only type definitions existed, with zero consumers. File-modification timestamps placed this roughly 44 minutes before this investigation, and `ListAgents` reported no other reachable session. Rather than guess at the cause, the content was read in full, judged sound and directly compatible with this unit's own requirements (matching, sometimes verbatim, the reasoning this unit would have produced independently — including the identical conclusion that a handoff must never default to a fabricated "passed" outcome), and used as the authoritative starting contract: the migration was revised to match its exact field set, and `store.rs`/`api.rs` were implemented against it rather than duplicating or discarding it. Nothing was deleted or overwritten to arrive at this decision.

## Remaining Gaps

- **No batch handoff.** Each handoff dispatches one test case as one single-task run. A future unit could add multi-case batch handoff (many `TaskDef`s in one `RunPlan`) if the product wants a "send N approved cases to one run" workflow — not built here because `domain.rs`'s own pre-existing `TestCaseHandoffRequest` contract (found during the mid-session discovery above) is single-case-shaped, and inventing a second, parallel batch contract wasn't justified by any stated requirement.
- **Validation is structural only**, not AI/quality-benchmark-scored (FRD-VAL-02's fuller sense) — because no model-gateway/LLM integration exists anywhere in this codebase. Disclosed in the handler's own doc comment; not silently overclaimed.
- **A generated case's execution outcome is always an explicit simulated result today** — there is no path from arbitrary generated text to a real, automatically-verified script. This is inherent to the product's current architecture (no codegen/model integration), not a shortcut taken in this unit, and is disclosed persistently in the UI.
- **No batch "Save to Project" progress UI beyond a spinner** — a save of many cases reports only aggregate success/failure counts, not per-case progress. Minor, non-blocking UX polish, not a correctness gap.
- Live, manual browser click-through of the full 19-step user journey (login → ... → audit trail) was **not performed** this session — no backend dev-server launch configuration exists in `.claude/launch.json` (only the frontend is configured), and the automated backend E2E integration test already exercises the identical real chain (generate → persist → validate → approve → handoff → real engine execution → traceability) with stronger, more precise assertions than a manual walkthrough would produce. This is disclosed honestly rather than fabricating a manual verification that didn't happen.

## AUD-005 Final Verdict

**CLOSED.**

Verified end-to-end, this session, via a real, passing, non-mocked integration test that exercises the actual repository code:
```
Generated Test Case (real POST, real persistence, verified by an independent
re-fetch — not an echo of the request)
        ↓
Real Validation (structural gate, a real 409 on failure, re-verified via GET)
        ↓
Real Execution Handoff (real RunPlan/TaskDef, explicit non-fabricated outcome,
routed through the exact same `create_run_core` the real Execution Hub uses)
        ↓
Existing Real Execution Engine (unmodified scheduler/engine/executor — the
real run reaches real COMPLETED, confirmed via `poll_run_state`)
        ↓
Real Result (task status independently re-fetched via `GET .../executions/:run_id`,
matches the real outcome; test case row shows `EXECUTED` with real
`executed_run_id`/`executed_task_id` pointing at that same real run/task)
```
This is not "CLOSED because a Run button exists" — every link in the chain above was independently asserted against real, re-fetched backend state in `aud005_full_generate_persist_validate_approve_handoff_execute_traceability`, not inferred from the existence of a UI control.

## Recommended Next Implementation Unit

No P0/P1 blocker exists, and this unit closed the one confirmed, precisely-located product-workflow gap the prior fresh audit identified. Per that audit's own established discipline (do not invent a unit merely to keep a sequence moving), the honest next step is a **fresh, evidence-based full-project audit**, not a pre-selected feature: this unit added a new table, six new routes, and a materially changed core page, and the correct way to decide what comes next is to re-run the same rigorous, fresh-evidence audit process against the *current* repository — exactly as the immediately-prior audit unit itself concluded — rather than assume this report's own account of what's now true.
