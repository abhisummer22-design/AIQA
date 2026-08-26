# AIQA — Phase 4 — FE-12 Status Vocabulary Migration Implementation Report

## 1. Status

**PASS.**

## 2. Exit Criterion

The brief's own completion criterion (§27, 15 items) is satisfied in full:

1. Every runtime status was inventoried and classified (§3 below).
2. Backend entity states are distinguished from frontend UI states (§7 below — six separate, never-merged domains).
3. Raw backend contracts are preserved — no backend enum, DB column, or wire value was renamed anywhere.
4. Canonical display vocabulary is documented (§7).
5. The one required inconsistent label was migrated (underscore → space display, §6/§8).
6. No blind synonym replacement occurred — every candidate merge was individually evaluated and rejected unless the two labels genuinely named the same state (§8, §17).
7. 401 and 403 remain distinct everywhere (verified across all 5 real resource pages, §10, §19).
8. Execution state (`RunState`) and test/task outcome (`TaskStatus`) remain two distinct domains (§7).
9. Finding/review state (`ReviewState`) remains distinct from execution state (§7).
10. FE-11's truthful unsupported-capability states (`NOT IMPLEMENTED` / `NO BACKEND CAPABILITY`) are untouched (§12).
11. Dashboard's design is preserved (§13).
12. Test Generator's design is preserved except one narrow, in-bounds label fix (§14).
13. The full FE-1–FE-11 test suite remains passing (§18/§21).
14. No backend file was modified (`git status --porcelain services/orchestrator/` is empty for this unit's entire duration, §23).
15. No fake status or fake fallback state was introduced (§16).

## 3. Status Inventory

A full read of `services/orchestrator/src/domain.rs` (the real backend's own enum definitions — priority #1 in this unit's own source-of-truth hierarchy) plus `forentend/src/types/contract.ts`, `forentend/src/types.ts`, `forentend/src/context/AuthContext.tsx`, `forentend/src/context/RealtimeContext.tsx`, and every page/component that renders a status, turned up exactly six distinct runtime-status domains in this app. No domain overlaps another; none needed merging.

| # | Domain | Owner type | Real values | Source |
|---|---|---|---|---|
| 1 | Execution (run) lifecycle | `RunState` | `CREATED, PENDING_APPROVAL, REJECTED, SCHEDULED, RUNNING, PARTIAL, COMPLETED, CANCELLED, FAILED` | `domain.rs::RunState` |
| 2 | Task/test outcome | `TaskStatus` | `QUEUED, RUNNING, PASSED, FAILED, BLOCKED, SKIPPED, CANCELLED, TIMEOUT, INFRA_ERROR, NEEDS_REVIEW` | `domain.rs::TaskStatus` |
| 3 | Finding/review lifecycle | `ReviewState` | `NEEDS_REVIEW, CONFIRMED, DISMISSED` | `store.rs`'s `bug_drafts.review_state` (free-text column; only these 3 values are ever written — confirmed via `review_bug_draft`'s own hardcoded `"CONFIRMED"`/`"DISMISSED"` and `intelligence.rs`'s auto-draft insert) |
| 4 | Authentication | `AuthStatus` (frontend-derived, no raw backend enum) | `no_key, key_present, checking, authenticated, unauthenticated, forbidden, offline, error` | `context/AuthContext.tsx` |
| 5 | Realtime transport connectivity | `RealtimeConnectionState` (frontend-derived) | `disconnected, connecting, connected, reconnecting, error` | `context/RealtimeContext.tsx` |
| 6 | Resource/page state (per real-data page) | untyped, per-page `ApiError.kind` branch | `loading, empty, unauthorized(401), forbidden(403), offline, generic-error` | Every real page (`DashboardPage`, `AuditPage`, `EvidencePage`, `ExecutionHubPage`, `ReportsPage`) |

Plus two domains that are explicitly **out of scope**, confirmed and left untouched:

| Domain | Owner | Why out of scope |
|---|---|---|
| FE-11 unsupported-capability state | literal strings `'NOT IMPLEMENTED'` / `'NO BACKEND CAPABILITY'` on 7 Case-C pages | Brief §17: preserve FE-11's truthful states verbatim; already internally consistent across all 7 pages (confirmed by `CaseCPages.test.tsx`, unchanged) |
| Audit `outcome` | `store.rs`'s free-text `audit_log.outcome` column | Real values are lowercase `"ok"`/`"denied"` — a third, correctly-separate domain (was this domain's own status already before this unit; no inconsistency found) |

Also inventoried and confirmed **not** real product vocabulary (Test Generator's own disclosed, protected, local-only simulation — brief §14/§19/§21, untouched except the one narrow fix in §14 below): `GeneratedTestCase['status']` (`PENDING_REVIEW, APPROVED, REJECTED, EXPORTED_TO_EXECUTION`), `SystemSettings`'s `apiTokenStatus`, `AppConnectionStatus`, and `TraceabilityGraph.tsx`'s single-word internal `stateCategory`.

## 4. Canonical Vocabulary Matrix

| Domain/entity | Raw value | Previous display label | Canonical display label | Source of truth | Reason for migration |
|---|---|---|---|---|---|
| RunState | `PENDING_APPROVAL` | `ExecutionHubPage.tsx`: literal `"PENDING_APPROVAL"` (underscore visible) | `"PENDING APPROVAL"` | `domain.rs::RunState` (raw, unchanged) + this unit's new `formatStatusLabel` | Underscore was a display artifact, not the raw contract; `ReportsPage`/legacy `StatusBadge.tsx` already used the space-separated convention — this unit made it the one, applied convention |
| TaskStatus | `NEEDS_REVIEW`, `INFRA_ERROR`, etc. | `ExecutionHubPage.tsx`: literal underscore | `"NEEDS REVIEW"`, `"INFRA ERROR"`, etc. | same | same |
| RunState (dropdown option text) | any multi-word state | `EvidencePage.tsx`'s run-picker: literal underscore | space-separated | same | same bug, same fix, same file class |
| Test Generator local review state | `EXPORTED_TO_EXECUTION` (etc.) | `GeneratedTestCasesView.tsx` line 837: literal raw enum text bypassing the file's own `getStatusBadge` | The file's own existing "Approved"/"Rejected"/"Dispatched to Runner"/"Pending Review" labels | `GeneratedTestCasesView.tsx`'s own pre-existing `getStatusBadge` (unchanged) | One stray call site skipped the file's own established formatter; not a new label, a bug fix reusing existing local vocabulary |
| Resource-error 401/403/offline/generic | n/a (frontend-derived) | Already byte-identical text across 5 pages, each with its own copy of the mapping | Unchanged text, now defined once | `state/StateViews.tsx`'s new `renderResourceErrorState` | Not a wording change — a duplication-elimination refactor (brief §10); confirmed behavior-identical by the full pre-existing test suite passing unchanged |

No other raw value, label, or spelling was changed anywhere in the app.

## 5. Backend Contract Verification

`services/orchestrator/src/domain.rs` was read directly (not inferred from documentation) for `TaskStatus`, `RunState`, `EventType`, and `SimOutcome`, including each enum's `as_db_str`/`from_db_str` and their `#[serde(rename_all = ...)]` attributes:

- `TaskStatus`/`RunState`/`EventType`: `SCREAMING_SNAKE_CASE` on the wire — matches `types/contract.ts`'s existing typed unions exactly, confirmed field-for-field.
- `SimOutcome`: `snake_case` on the wire (`pass`/`fail`/`timeout`/`infra_error`) — a **different** serialization convention from the other three, on the same backend, already correctly reflected in `types/contract.ts`'s `TaskDef.simulate.outcome` type (an FE-8 fix, confirmed still correct, untouched).
- `store.rs`'s `bug_drafts.review_state` / `audit_log.outcome`: free-text `TEXT` columns, not Rust enums — their real, actually-written values were confirmed by reading `review_bug_draft`, `intelligence.rs`'s auto-draft insert, and every `audit(...)` call site in `api.rs` directly, not assumed.
- Transitions: `RunState::is_terminal()`/`TaskStatus::is_terminal()`/`TaskStatus::satisfies_dependents()` read directly; the frontend's own `TERMINAL_RUN_STATES` set in `ExecutionHubPage.tsx` (pre-existing, unchanged) already mirrors `RunState::is_terminal()` exactly.
- No contract value was changed. No migration script, no DB write, no `services/orchestrator` file touched.

## 6. Frontend Migration

Files changed by this unit (11 total — see §9 for the shared architecture and §7 for the domain separation these changes implement):

- **`forentend/src/utils/statusLabel.ts`** (new) — the one shared `formatStatusLabel(raw): string` function (underscore → space; nothing else).
- **`forentend/src/components/state/StateViews.tsx`** — added `renderResourceErrorState(error, onRetry, copy)`, the centralized 401/403/offline/generic-error → `StateCard` mapping.
- **`forentend/src/components/pages/ExecutionHubPage.tsx`** — its `RunState`/`TaskStatus` `Badge` labels now go through `formatStatusLabel`; its local `errorStateCard` now delegates to `renderResourceErrorState`; six now-unused icon imports removed.
- **`forentend/src/components/pages/EvidencePage.tsx`** — its run-picker `<option>` label now goes through `formatStatusLabel`; its `errorStateCard` now delegates to the shared function.
- **`forentend/src/components/pages/AuditPage.tsx`**, **`DashboardPage.tsx`**, **`ReportsPage.tsx`** — each page's local 401/403/offline/generic switch replaced with one call to `renderResourceErrorState`, passing only the genuinely page-specific copy (forbidden title/description, offline suffix, generic title).
- **`forentend/src/components/test-generator/GeneratedTestCasesView.tsx`** — one line (`{activeTestCase.status}` → `{getStatusBadge(activeTestCase.status)}`), reusing that file's own pre-existing formatter instead of bypassing it.
- **Three new/updated test files** — see §18.

Nothing else in `forentend/src` was modified by this unit (FE-11's own, separately-completed changes to `AIQAContext.tsx`, `routeConfig.tsx`, `types.ts`, the 7 Case-C pages, `Header.tsx`, `Sidebar.tsx`, `CommandPalette.tsx`, and `RunTriggerModal.tsx`'s deletion predate this unit and were left untouched here).

## 7. Domain Separation

Each of the six real domains (§3) keeps its own type, its own value set, and — critically — its own color/icon mapping. None was collapsed into another:

- **Execution lifecycle vs. test outcome**: `ExecutionHubPage.tsx` keeps two separate `Record` maps, `RUN_STATE_STYLE` (9 keys) and `TASK_STATUS_STYLE` (10 keys), never merged — a run can be `COMPLETED` while its tasks show a `FAILED`/`NEEDS_REVIEW` mix, and the UI displays both independently, exactly as `RunSummary`'s own per-status counters (`passed`, `failed`, `blocked`, `needs_review`, ...) already model it.
- **Finding/review lifecycle**: `ReportsPage.tsx`'s `reviewStateBadgeClass`/`severityBadgeClass` are local to `Finding`/`FailureCluster` and were never proposed to share code with `RunState`/`TaskStatus` — a `DISMISSED` finding is not, and is never displayed as, a `CANCELLED` execution.
- **Authentication vs. connectivity**: `AuthStatus` (session-level: "is this key accepted by a real protected request") and `RealtimeConnectionState` (transport-level: "is the WebSocket currently open") are computed independently, by different contexts, and shown by different UI elements — Header's compact pill reflects `AuthStatus` only ("Orchestrator Online" / "Not Confirmed Online"); `ExecutionHubPage`'s own indicator reflects `RealtimeConnectionState` only ("Live" / "Realtime Off" / "Connecting…" / "Reconnecting…" / "Realtime Error"). Reviewed explicitly for confusion risk (§17) — neither ever borrows the other's wording.
- **Resource/page state**: `loading`/`empty`/`unauthorized`/`forbidden`/`offline`/generic-error is a `ApiError.kind`-driven state that exists once per real page, now centralized (§9) but still conceptually separate from all of the above — an `unauthorized` Dashboard fetch says nothing about whether a specific execution is `RUNNING`.
- **Unsupported capability**: FE-11's `NOT IMPLEMENTED`/`NO BACKEND CAPABILITY` strings on the 7 Case-C pages are literal, static, page-header text — never generated from, or compared against, any of the five typed status domains above.

## 8. Deprecated Vocabulary

| Label | Where | Classification | Disposition |
|---|---|---|---|
| Literal `"PENDING_APPROVAL"` / `"NEEDS_REVIEW"` / `"INFRA_ERROR"` (underscore visible) | `ExecutionHubPage.tsx`, `EvidencePage.tsx` | Casing/spelling inconsistency (not semantically wrong, not misleading — just inconsistent with the majority display convention already used elsewhere) | Migrated to space-separated display via `formatStatusLabel`; raw value unchanged everywhere |
| Raw `{activeTestCase.status}` bypassing `getStatusBadge` | `GeneratedTestCasesView.tsx` | Duplicate/inconsistent presentation of the *same file's own* vocabulary (one call site didn't use the local formatter two other call sites already used) | Fixed to call the existing formatter; no new label invented |
| 5 duplicated `errorStateCard`/inline-switch implementations | `AuditPage.tsx`, `EvidencePage.tsx`, `ExecutionHubPage.tsx`, `ReportsPage.tsx`, `DashboardPage.tsx` | Not a wording inconsistency (the five copies were already byte-identical for `unauthorized`/`offline`'s core sentence and for every branch's icon/statusLabel/color) — a code-duplication risk for *future* drift | Centralized into `renderResourceErrorState`; zero wording changed |

No status was renamed. No two genuinely different states were merged. No misleading UI wording was found and left unfixed.

## 9. Shared Status Architecture

Two new shared pieces, both intentionally minimal:

- **`utils/statusLabel.ts`** — a single 3-line function. Not a status-to-color map, not a per-domain formatter registry — just the one transform every multi-word raw value needs. Deliberately not extended to also handle color/icon, since those are legitimately domain-specific (§7) and a shared color map would have forced `RunState`'s and `TaskStatus`'s already-distinct, already-correct color choices into one scheme.
- **`state/StateViews.tsx`**'s **`renderResourceErrorState`** — centralizes exactly the part that was already identical (the four-branch shape, all four `statusLabel`s, all four icons/colors, the fully-identical `unauthorized` and default-offline text) while keeping the part that is genuinely page-specific (the `forbidden` title/description — which correctly differs because different endpoints require different real roles — and the offline/generic-error page-naming) as required parameters, per `ResourceErrorCopy`.

**Not centralized, on purpose**: `RUN_STATE_STYLE`/`TASK_STATUS_STYLE` (ExecutionHubPage), `reviewStateBadgeClass`/`severityBadgeClass` (ReportsPage), `outcomeBadgeClass` (AuditPage), and the Dashboard's own `ProjectStatus` badge — each has only 2–10 entries, each is domain-specific, and merging any pair of them would violate this unit's own explicit "do not merge unrelated statuses" rule for a purely cosmetic DRY win. This matches the brief's own "do not over-engineer... centralize only genuinely shared vocabulary" instruction.

## 10. 401 / 403

Both are, and remain, fully distinct in every one of the 5 real resource pages plus the app-wide `AuthContext`/`StateViews` auth-gate layer:

- **401** (`ApiError.kind === 'unauthorized'`): `statusLabel="AUTHENTICATION REQUIRED · 401"`, title "Authentication required", description "The orchestrator rejected this request. The API key may have been revoked since it was last verified." — identical text everywhere it appears (both the per-resource `renderResourceErrorState` and the app-wide `StateViews.tsx::UnauthorizedState`, which independently uses the same "Authentication required" title for its own, unrelated auth-gate purpose — the two are structurally different components serving different moments, never conflated).
- **403** (`ApiError.kind === 'forbidden'`): `statusLabel="ACCESS FORBIDDEN · 403"`, page-specific title/description that correctly names the actual required role where the backend specifies one (e.g. Audit's real "(the backend requires Operator role or above)" — Reports' viewer-key 403 during manual verification returned the real backend detail *"role 'viewer' cannot perform this action (requires 'operator')"*, confirming the real distinction still holds end to end).
- Live-verified this unit (§19): a genuine 403 from a viewer-scoped key attempting to create a `maestro`-executor run rendered the forbidden path with the real backend's own message, never confused with a 401.

## 11. Offline / Error

Unchanged wording, now defined once: `statusLabel="ORCHESTRATOR UNREACHABLE"` / title "Orchestrator unreachable" / `WifiOff` icon for `kind === 'offline'`; `statusLabel="ERROR"` / generic (page-specific default) title / `error.message` for everything else. Never conflated with 401/403 — `renderResourceErrorState`'s `switch` keeps all four branches structurally separate, matching the pre-existing per-page implementations exactly.

## 12. Unsupported Features

Confirmed intact and untouched: all 7 FE-11 Case-C pages (`AIQAOrchestratorPage`, `TestCaseValidatorPage`, `RBACTestingPage`, `APITestingPage`, `MobileTestingPage`, `CustomTestingPage`, `FeatureMapFlowPage`) still render `NOT IMPLEMENTED` / `NO BACKEND CAPABILITY` verbatim — confirmed by `CaseCPages.test.tsx` (30 tests, unmodified, all still passing) and by live-checking `/api-testing` this unit (§19), which still shows the exact FE-11 copy with its real navigation CTA to Execution Hub.

## 13. Dashboard Preservation

No layout, markup, or visual change. The only Dashboard edit was replacing its local 4-branch `unauthorized`/`forbidden`/`offline`/default switch with one call to `renderResourceErrorState` carrying the exact same four strings it already used — confirmed identical by `DashboardPage.test.tsx`'s pre-existing items 7/8/10 (401/403/generic-error assertions) still passing unchanged, and by live manual verification (§19) showing the real KPI cards, donut chart, and Orb exactly as before.

## 14. Test Generator Preservation

Untouched except the one line described in §4/§6/§8: `GeneratedTestCasesView.tsx`'s sticky review-footer now calls that same file's own pre-existing `getStatusBadge` instead of printing the raw enum. No new label was invented, no local simulation logic was touched, `TraceabilityGraph.tsx` and `ApplicationSourceSection.tsx`'s own disclosed local-simulation vocabulary (`Math.random()` ping delay, `'Simulate real asynchronous file ingestion'`) are unmodified.

## 15. Realtime Preservation

`RealtimeContext.tsx` was read for inventory purposes only — zero lines changed. No new WebSocket connection was created; `ExecutionHubPage.tsx`'s existing single `useRealtime().subscribe` call is untouched. Live-verified this unit (§19): approving the real `PENDING_APPROVAL` run transitioned it to `RUNNING` on screen via the existing realtime subscription, with no manual refresh.

## 16. AIQAContext

No status-related change. `AIQAContext.tsx` was not edited by this unit at all (FE-11 had already pruned every fabricated status/state field from it in the prior unit; nothing remained to touch here). No fake status field, no new context, no reintroduced fabricated state.

## 17. Search Verification

Ran the brief's own suggested searches (`grep -Rni` for `cancelled/canceled`, `in progress`, `running`, `active`, `completed/complete`, `success`, `passed`, `failed`, `error`, `unauthorized`, `forbidden`, `not implemented`, `unsupported`) across `forentend/src`, plus targeted greps for every raw multi-word status literal (`\{[a-zA-Z.]+\.(status|state|review_state)\}`) and for `StatusBadge`/`FailureClassificationBadge` import sites. Findings, all individually classified:

- **`Cancelled`/`Canceled`/`CANCELED`**: zero matches anywhere — no spelling inconsistency exists (everything is `CANCELLED`, matching the real backend).
- **`IN_PROGRESS`**: one match, `TraceabilityGraph.tsx` — Test Generator's own internal (never literally rendered as text) step-category flag, distinct domain, correctly left alone.
- **`SUCCESS`**: zero matches — no such literal status exists in this app.
- Header's "Orchestrator Online" vs. `StateViews.tsx`'s "CONNECTED" (both describing `AuthStatus === 'authenticated'`): reviewed and kept as two intentionally different phrasings for two different UI contexts (a compact persistent chrome badge vs. a full descriptive state card) — neither contradicts the other, and this unit's own rule against "blind synonym replacement" applies here: collapsing a badge's necessarily-terse copy into a full card's descriptive copy would not improve truthfulness, only remove contextually-appropriate brevity.
- Dashboard's "LIVE TELEMETRY" / "REAL DATA" (driven by `RealtimeConnectionState === 'connected'`) vs. `ExecutionHubPage`'s "Live"/"Realtime Off"/etc. (the same enum, five states): reviewed — Dashboard's badge answers a different question ("is this chart section live-streaming or a fetched snapshot") than Execution Hub's ("is the transport itself connected, with a Reconnect affordance"); both are honest, neither claims the other's meaning, and unifying them would mean either adding a Reconnect affordance to Dashboard (a real feature change, explicitly out of scope — "do not redesign Dashboard") or removing Execution Hub's more detailed states (a regression). Left as two legitimately distinct, honest treatments of the same underlying enum.
- `AIQAProject`/`ProjectStatus` (Dashboard's `'Healthy'`/`'Running'` project-summary indicator): confirmed still a distinct, already-disclosed-since-FE-10 frontend-derived rollup, not a raw backend value, no underscore risk (both are single words), left unchanged.
- The orphaned `common/StatusBadge.tsx`/`FailureClassificationBadge` component: confirmed still fully unused (zero import sites anywhere in `forentend/src`, re-verified this unit). Not deleted — deletion is dead-code removal, not vocabulary migration, and this unit's own brief explicitly says "FE-12 is NOT... a fake-data removal phase." Not reused as-is either, since wiring it into `ExecutionHubPage` would have swapped that page's already-fine, already-tested `TASK_STATUS_STYLE` colors for `StatusBadge`'s different color choices — an unwanted visual change this unit's "no broad visual redesign" rule forbids. Left exactly as found; flagged here for visibility.

## 18. Tests

**251 pre-existing tests: unchanged, all still passing.** **10 new tests added, all passing.** Total: **261/261**, 14 test files.

New/updated:

- **`forentend/src/utils/statusLabel.test.ts`** (new, 3 tests) — `formatStatusLabel` replaces underscores with spaces; leaves single-word statuses unchanged; never alters character content (round-trip check) — guards against a future "helpful" rename creeping into this function.
- **`forentend/src/components/state/StateViews.test.tsx`** (+4 tests) — `renderResourceErrorState`: 401 vs. 403 render distinct, non-interchangeable labels/titles/descriptions; the offline branch uses the page-specific suffix when given and the plain sentence when omitted; the generic branch uses the page-specific title when given and the established default otherwise; `request_id` renders only when the real error actually carried one.
- **`forentend/src/components/pages/ExecutionHubPage.test.tsx`** (+3 tests, under a new `FE-12 — canonical status vocabulary` describe block) — a real multi-word `RunState` (`PENDING_APPROVAL`) renders as "PENDING APPROVAL", never with a literal underscore; a real multi-word `TaskStatus` (`NEEDS_REVIEW`) renders the same way in the task table; a real 401 and a real 403 render distinct, non-interchangeable state cards on this page specifically.

All pre-existing tests across `DashboardPage.test.tsx`, `AuditPage.test.tsx`, `EvidencePage.test.tsx`, `ReportsPage.test.tsx`, `CaseCPages.test.tsx`, `AppRoutes.test.tsx`, `ProjectContext.test.tsx`, `ProjectSelector.test.tsx`, `RealtimeContext.test.tsx`, and `aiqaApi.test.ts` pass unchanged — none needed modification, confirming the centralization refactor (§9) was genuinely behavior-preserving.

## 19. Manual Verification

Performed live against the real backend (not simulated). Restarted the real orchestrator (`cargo run --release`, healthy on `:8080`) and the frontend dev server, minted fresh real `fe12-manual-verify-operator`/`fe12-manual-verify-viewer` API keys scoped to the real "AIQA Core Platform" project.

- **Manually verified, live, this unit:**
  - **Scenario A (real execution states)**: created a genuinely real execution with a `maestro` executor (which the real policy engine gates behind approval) — the run rendered a real `PENDING_APPROVAL` `RunState`, and the badge showed **"PENDING APPROVAL" with a space, confirmed live, not the previous literal underscore**. Clicked the real "Approve" button — the run transitioned to a real `RUNNING` state on screen via the existing realtime subscription (no manual refresh), and its task showed the real `QUEUED`/`RUNNING` `TaskStatus`.
  - **Scenario C/D (401/403)**: attempted the same `maestro` creation first with the viewer-scoped key — received and correctly rendered a real, live 403 with the backend's own message ("role 'viewer' cannot perform this action (requires 'operator')"), distinct from the 401 card (confirmed no "AUTHENTICATION REQUIRED" text appeared).
  - **Findings (Reports)**: real findings list still rendered `NEEDS REVIEW`/`DISMISSED` space-separated correctly after the centralization refactor.
  - **Audit**: the real audit trail showed this session's own real `run.create`/`run.approve` rows with the real `"OK"` outcome and the real policy-note detail text, confirmed live.
  - **Scenario F (unsupported pages)**: spot-checked `/api-testing` — FE-11's exact `NOT IMPLEMENTED`/`NO BACKEND CAPABILITY` copy and its real "Go to Execution Hub" CTA render unchanged.
  - **Scenario I (project switch)**: implicit in the above — the real project (`AIQA Core Platform`) stayed correctly scoped across every page visited; no cross-project label leakage observed.
  - Browser console: zero unexpected errors — the only logged error was the deliberately-triggered real 403 above.
- **Verified via automated test only (not re-triggered live this unit)**: the offline/`ApiError('offline', ...)` branch of `renderResourceErrorState` (already live-verified pre-refactor in the FE-11 unit for byte-identical code paths; covered here by `StateViews.test.tsx`'s new offline-branch test and by every pre-existing page test's own offline case, all passing unchanged).
- **Verified via code inspection only**: `TIMEOUT`/`INFRA_ERROR` `TaskStatus` label formatting (no live task reached either state during this session's manual pass; covered by `formatStatusLabel`'s unit test and by `TASK_STATUS_STYLE`'s complete `Record<TaskStatus, string>` type coverage, which the TypeScript compiler itself enforces is exhaustive).
- No manual verification claim above describes anything that did not actually happen this session.

## 20. Typecheck / Build

- `npx tsc --noEmit` — clean after every edit batch in this unit.
- `npx vitest run` — 261/261 passing, 14 files.
- `npx vite build` — succeeds (`dist/` produced; the pre-existing >500kB chunk-size warning is unrelated to this unit).

## 21. Regression

Zero. All 251 pre-existing tests (FE-1 through FE-11 coverage) pass unmodified. Live manual verification confirmed Dashboard, Execution Hub, Audit, Reports, and a Case-C page all still function and render correctly end to end against the real backend.

## 22. Documentation Compliance

**The three product documents (PRD+FRD+SRD v6.0, Business Logic & Architecture v2.0, End-to-End Flow Diagram v6.0) and the API Contract v1.0/Acceptance Matrix v1.0 are now present in the repository** (added between the FE-11 and FE-12 units — every FE-1 through FE-11 report's "no such documents exist" statement was accurate at the time it was written, per each report's own re-confirmation). This unit read all of them:

- **SRD §13 "Common Execution Contract"** and **Business Logic §10** both list the canonical execution outcome set as `QUEUED, RUNNING, PASSED, FAILED, BLOCKED, SKIPPED, CANCELLED, NEEDS_REVIEW, and infrastructure/environment error` — matching the real backend's `TaskStatus` exactly, except that the real backend splits "infrastructure/environment error" into two distinct real states, `TIMEOUT` and `INFRA_ERROR` (confirmed by reading `domain.rs` directly), which the product documents' prose summary does not call out as two separate values. This is not a contradiction requiring resolution — the real backend is simply more granular than the documents' summary sentence, and per this unit's own source-of-truth hierarchy (real backend implementation first), both real states are preserved as-is.
- **`AIQA_API_CONTRACT_v1.0.md` §13 "Canonical Execution Status"** explicitly names `INFRASTRUCTURE_ERROR` as canonical and states "Frontend MUST NOT create alternate spellings for these statuses" — but the real, already-implemented backend enum is `INFRA_ERROR` (confirmed directly in `domain.rs`, and already flagged by `types/contract.ts`'s own pre-existing doc comment: *"Note: still `INFRA_ERROR`, not the contract's `INFRASTRUCTURE_ERROR` — that rename is FE-12... do not silently alias it here"*). **This is a genuine, pre-existing (not introduced by this unit) drift between the aspirational written contract and the actual shipped backend.** Per this unit's own explicit rules ("do not change backend contracts," "if backend terminology itself is inconsistent, document the issue, do not silently change it"), the raw value was left exactly as the real backend defines it (`INFRA_ERROR`) everywhere — transport, types, tests, and every rendered instance. This is documented here, not silently resolved.
- **§3.2 of `AIQA_PHASE4_DOCUMENTATION_COMPLIANCE_GAP_ANALYSIS.md`** (added alongside the three product PDFs) had already flagged that `NEEDS_REVIEW` is a documented, real product state — confirmed still true and already correctly wired (Findings' `review_state`, `TaskStatus.NEEDS_REVIEW`) with no further action needed.
- The written API Contract's §32 "Current Implementation Status" vocabulary (`NOT_STARTED, IN_PROGRESS, IMPLEMENTED, TESTED, VERIFIED, BLOCKED`) is a **documentation/process-tracking** status domain (tracked in `AIQA_ACCEPTANCE_MATRIX_v1.0.md`), never rendered anywhere in the running application — confirmed out of this unit's runtime-vocabulary scope by inspection, not assumption.

Where the documents and the real implementation agreed, the real implementation's exact wording was used. Where they disagreed (`INFRA_ERROR` vs. `INFRASTRUCTURE_ERROR`), the real implementation won per this unit's own stated hierarchy, and the disagreement is documented rather than silently resolved.

## 23. Backend Changes

**None.** `git status --porcelain services/orchestrator/` was empty for this entire unit. `domain.rs`, `store.rs`, `api.rs`, and every other backend file were read for inventory purposes only.

## 24. Remaining Vocabulary Exceptions

Every remaining status synonym/variant, and why it remains:

- **Header's "Orchestrator Online"/"Not Confirmed Online" vs. `StateViews.tsx`'s "CONNECTED"/other `AuthStatus` card titles** — same underlying `AuthStatus`, deliberately different phrasing for a compact persistent badge vs. a full descriptive card; neither is inaccurate; documented in §17.
- **Dashboard's "LIVE TELEMETRY"/"REAL DATA" vs. Execution Hub's "Live"/"Realtime Off"/"Connecting…"/"Reconnecting…"/"Realtime Error"** — same `RealtimeConnectionState` enum, deliberately different granularity (binary vs. five-state) because the two badges answer different questions in different contexts; documented in §17; unifying either direction would require a real feature/UX change to Dashboard, out of this unit's "no redesign" scope.
- **`INFRA_ERROR` (real, everywhere) vs. `INFRASTRUCTURE_ERROR` (the written API Contract's aspirational name)** — pre-existing documentation-vs-implementation drift, not introduced by this unit, not silently resolved; documented in §22.
- **The orphaned, unused `common/StatusBadge.tsx`** — a complete, correct, but currently-unreachable status→label/color/icon mapping for the exact `TaskStatus`-shaped vocabulary; left in place undeleted (dead-code removal is out of this unit's stated scope) and not reused (would have forced an unwanted color change on Execution Hub); documented in §17.
- **Test Generator's own local vocabulary** (`GeneratedTestCase['status']`, `TraceabilityGraph.tsx`'s internal step categories, `AppConnectionStatus`) — intentionally distinct, protected, disclosed-fake domains per brief §14/§19/§21; no real-vocabulary overlap risk since none of these values are ever compared against, or rendered alongside, real `RunState`/`TaskStatus`/`ReviewState` data.

No two labels found to genuinely represent the *same* state were left un-migrated.

## 25. Limitations

- `TIMEOUT`/`INFRA_ERROR` `TaskStatus` label rendering was verified by code inspection and by `formatStatusLabel`'s exhaustive unit test, not by driving a real task into either state live this session (no real executor adapter exists to reliably produce a real timeout on demand within this session's time budget; `TaskStatus` is a TypeScript-enforced exhaustive `Record`, so a missing case would be a compile error, not a silent gap).
- The `RUNNING` state created during manual verification (the real `maestro`-executor run, "FE-12 vocabulary verification run") was intentionally left running rather than force-terminated, matching the established precedent in this database of prior FE units' own real manual-verification runs being left as evidence rather than cleaned up.

## 26. Out of Scope

**No FE-13 (or any post-FE-12) work was started.** This unit touched only the 11 files listed in §6, added only the vocabulary-formatting/centralization architecture described in §9, and made no backend, database, or design-system change beyond what is documented above.
