# AIQA — Phase 4 — FE-9 Implementation Report

## 1. Status

**PASS**

## 2. Exit criterion

FE-9's brief: replace FE-8's bounded-polling execution monitoring with the backend's real realtime/event mechanism where the backend and product documentation support it — one centralized connection, real event contract, real project isolation, real reconnect handling, and event-driven (not polled) updates to Execution Hub, with disclosed, honest treatment of anything the real backend does not actually support. Satisfied in full: `RealtimeContext.tsx` now owns a single, centralized, project-scoped WebSocket connection to the real backend; `ExecutionHubPage`, `EvidencePage`, and `AuditPage` all consume it; FE-8's `setTimeout`-based poll loop is gone entirely (§15); and two real, live-discovered bugs (one in this sprint's own new code, one in the pre-existing dev-proxy infrastructure) were found, root-caused, and fixed during manual verification, not merely noted.

## 3. Source documentation

Per brief §2, the three now-present documents were re-read specifically for realtime/eventing content before any code was written:

- `AIQA_Consolidated_PRD_FRD_SRD_v6.0...` — §9 (`FRD-RT-01..04`: normalize worker events, update dashboards incrementally, reconcile authoritative state after reconnect, link evidence to run/task/test/finding/report), §11 (Logical Architecture's Observability row names `WebSocket` explicitly), §13 (Common Execution Contract's outcome vocabulary), §15 (Performance & Real-Time).
- `AIQA_Business_Logic_Architecture_v2.0...` — §10 ("Events update the UI incrementally; reconnect reconciles authoritative state and processing is idempotent"), §15 ("Real-time events for execution, RBAC and API telemetry").
- `AIQA_Complete_End_to_End_Flow_Diagram_v6.0.pdf` — page 5 ("EVENT BUS / REDIS ... WEBSOCKET ... DASHBOARD"), page 6 ("OBSERVABILITY: ... WebSocket real-time updates").

**A real citation mismatch, disclosed rather than silently resolved (brief §2's own instruction)**: the backend's own source comments cite section labels — "SRD §5", "SRD §6 Real-Time Data Flow", "SRD §18 reconnect path", "Contract §14", "ACC-008" — that do not literally exist in this now-added consolidated v6.0 document (whose own numbering is §9/§11/§13/§15/§18 Acceptance Scenarios, with different content under each number). This means the backend was originally built against an earlier, more granular SRD/API-Contract document that is not present in this repository, only referenced by name in code comments. The *substance* of what those comments describe (event normalization, incremental UI updates, reconnect reconciliation, idempotent processing) matches the consolidated document's real content exactly — this is a citation-label mismatch, not a functional contradiction, and is not something this sprint attempted to "fix" (there is nothing to fix; both describe the same real system).

**A real scope gap, also disclosed rather than acted on**: the Flow Diagram (page 5) and Business Logic §15 both depict/describe realtime reaching the Dashboard, RBAC, and API Testing surfaces — but the real backend emits realtime events only for executions/tasks (§4). No dashboard-KPI event, RBAC event, or API-telemetry event exists anywhere in the real backend. Since Dashboard/RBAC/API Testing remain 100% fabricated `AIQAContext` pages (undisturbed by every prior FE unit, and untouched here too — §20/§21), and no real backend mechanism could feed them anyway, this sprint does not wire realtime into any of them. This mirrors FE-8's own gap-analysis precedent (`AIQA_PHASE4_DOCUMENTATION_COMPLIANCE_GAP_ANALYSIS.md`) of disclosing a documentation-vs-implementation gap rather than reactively "fixing" it mid-sprint.

## 4. Backend realtime capability

Confirmed by reading `services/orchestrator/src/api.rs`, `domain.rs`, `engine.rs`, and `ws_tests.rs` directly — not assumed from a route name:

**Transport**: a real WebSocket (axum's `WebSocketUpgrade`), not SSE or any other mechanism. Two endpoints exist:

- `GET /runs/:id/ws?key=` — legacy, per-run. Replays the run's full durable event backlog on connect, then streams new events. Not used by this sprint (superseded by the project-scoped stream below, per BE-14's own design).
- `GET /api/v1/projects/:project_id/events?key=` — the real, project-scoped stream (BE-14/Contract §14) this sprint uses. **No backlog replay** — delivers only events occurring after a successful subscription.

**Authentication**: a `?key=` query parameter, not the `Authorization` header every REST call uses — confirmed as a deliberate, documented backend design choice (`ws_handler`'s own doc comment: "browsers cannot set custom headers on a WebSocket handshake"), not a weaker mechanism this sprint invented. `projectEventsWsUrl()` (`services/aiqaApi.ts`) reads the exact same raw key `getApiKey()` already returns — no second credential store (brief §5's explicit permission for this exact case).

**Authorization**: `require_project_access` — a project-scoped key requesting a different project's stream gets a real pre-upgrade 403; a nonexistent project gets 404; a missing/invalid key gets 401. Confirmed via `ws_tests.rs`'s real TCP-socket-level tests (`be14_missing_key_is_rejected`, `be14_nonexistent_project_is_rejected`, `be14_cross_project_subscription_is_rejected`) and live during manual verification (§23, Scenario H).

**A real, disclosed browser platform limitation ("G-WS", the backend's own name for it)**: pre-upgrade rejections are real, plain-text HTTP responses server-side — but the browser's native `WebSocket` API does not expose a failed handshake's HTTP status to JavaScript at all (only a generic `error` then `close` event, per the WHATWG spec). This frontend therefore cannot distinguish "wrong key" from "wrong project" from "server down" from a failed WS connection alone. Documented in `RealtimeContext.tsx`'s own module doc comment and not worked around with an invented signal.

**Lag handling**: a subscriber that falls behind the server's 1024-capacity broadcast channel is closed with a real, distinct WebSocket close code `4001` and reason string ("lagged behind... reconnect and catch up via REST") — confirmed via `ws_tests.rs::be14_lagging_subscriber_is_closed_with_distinct_code_not_silently_skipped`.

## 5. Event contract

**No new type was invented.** `RunEvent`/`EventType` (`types/contract.ts`, unchanged since FE-1) are already an exact, field-for-field mirror of `domain.rs::RunEvent`, re-verified this sprint:

```ts
interface RunEvent {
  event_id: string;        // real UUID v4 — the real dedup key
  run_id: string;
  task_id: string | null;
  sequence: number;        // real per-run monotonic counter
  timestamp: string;
  schema_version: number;
  type: EventType;         // wire field is `type`, renamed from Rust's `event_type`
  status: TaskStatus | null;
  summary: string | null;
  project_id: string | null;
  project_sequence: number | null;  // only non-null on this project stream
}
```

**Real vocabulary actually emitted** (confirmed by reading every `record.emit(...)` call site in `engine.rs`, not assumed from the enum's own name list): `RUN_SCHEDULED` (once, when a run actually starts executing — `record.state` is already `Running` by the time this fires, whether at creation or after approval), nine `TASK_*` events, and `RUN_COMPLETED` (once, terminal, for *every* way a run ends).

**A real, structurally significant limitation, discovered by reading the emit sites rather than assumed**: there is no dedicated approval/rejection/cancellation event type. `reject_run` piggybacks its terminal signal onto the same `RUN_COMPLETED` type (distinguishable only by its `summary` string, e.g. `"run rejected by ..."`); cancellation is visible only as per-task `TASK_CANCELLED` events followed by the same unified `RUN_COMPLETED`; and `RUN_COMPLETED` never carries the run's actual final `RunState` (`status` is `null` on that event). A consumer that needs to know COMPLETED vs. PARTIAL vs. FAILED vs. CANCELLED vs. REJECTED must re-fetch. This directly shaped §10/§15 below — it is not an oversight that this sprint refetches rather than trusting the event payload alone.

## 6. Realtime architecture

`forentend/src/context/RealtimeContext.tsx` (new) — the single centralized connection, matching brief §7/§8 exactly:

```ts
interface RealtimeContextValue {
  connectionState: 'disconnected' | 'connecting' | 'connected' | 'reconnecting' | 'error';
  lastEventAt: string | null;
  lastError: Error | null;
  reconnect: () => void;
  subscribe: (listener: (event: RunEvent) => void) => () => void;
}
```

Deviates from the brief's own illustrative interface (which suggested raw `connect()`/`disconnect()`) in one deliberate way: connect/disconnect are driven entirely by `AuthContext.status` + `ProjectContext.selectedProjectId` internally (matching how `ProjectContext` itself never exposes raw fetch control either) — `reconnect()` is exposed only for a manual retry (an error-state `Cta`), not general-purpose control. The brief's own text explicitly permits this ("Exact shape must follow the actual project/backend needs").

Internals: a single `WebSocket` held in a ref, a single reconnect-timeout ref, a bounded reconnect-attempt counter, an `event_id` dedup `Set` (bounded to 500 entries), and a `Set` of subscriber callbacks. `handleIncoming` validates shape, checks project isolation, deduplicates, then synchronously calls every subscriber — a throwing subscriber is caught and logged, never propagated (brief §10/§32). `ExecutionHubPage`/`EvidencePage`/`AuditPage` each call `useRealtime().subscribe(listener)` — no page ever constructs its own `WebSocket`.

## 7. Authentication integration

No second auth mechanism (brief §5, critical rule 5). The connect effect is exactly:

```ts
const shouldConnect = auth.status === 'authenticated' && !!project.selectedProjectId;
```

Every other `AuthStatus` (`no_key`, `key_present`, `checking`, `unauthenticated`, `forbidden`, `offline`, `error`) tears the connection down. This is a deliberate simplification, disclosed rather than silent: unlike `ProjectContext`, which preserves its (inert, cheap-to-restore) project list across `offline`/`error` because those states don't prove the credential is actually invalid, a live socket is a stateful resource — tearing it down on any non-`authenticated` status and letting it cleanly reconnect once `authenticated` is reproven is simpler to reason about and test than trying to distinguish "backend genuinely down" from "one unrelated validation call bounced." Verified live and by automated test (items 1-2, 6-7).

## 8. Project-scope integration

`useProject()` (FE-5, unchanged) is the only source of scope — never a query parameter, never a second selector. The connect effect is keyed on `[auth.status, project.selectedProjectId]`; `openSocket` always tears down any existing socket before opening a new one, so a project switch structurally cannot produce two live sockets. Live-verified (§23, Scenario H) with a genuine second real project (an admin-scoped key was minted specifically to exercise this, since the operator key used throughout FE-8 was itself project-scoped and could not see a second project — see §27): switching "AIQA Core Platform" → "Payments Gateway QA" opened exactly one new real `WebSocket` (captured via a live constructor-wrapping instrumentation, not inferred) to the new project's URL, a real run created in the new project appeared correctly, and switching back showed Project A's full original run list with zero cross-project leakage in either direction.

## 9. Connection states

| State | Meaning | Trigger |
|---|---|---|
| `disconnected` | No connection, none intended | `no_key`/`unauthenticated`/etc., or no project selected |
| `connecting` | First attempt for a fresh selection | `openSocket(..., isReconnectAttempt: false)` |
| `connected` | Real `onopen` fired | — |
| `reconnecting` | Backoff timer pending, or an attempt is retrying | `scheduleReconnect` / `openSocket(..., true)` |
| `error` | `MAX_RECONNECT_ATTEMPTS` (6) exhausted | 7th consecutive close without an intervening `onopen` |

Surfaced as a small badge (`REALTIME_STATE_STYLE`/`REALTIME_STATE_LABEL`, `ExecutionHubPage.tsx`) reusing the same `Badge` component every other status already uses — not a new visual system (brief §23: "do not make connection state more visually prominent than the actual product workflow requires"). An `error`-state badge shows a small "Reconnect" link calling `realtime.reconnect()`. Live-verified in full, including exhaustion, via real server-initiated closes (§23, automated test items 8-10).

## 10. Execution realtime

`ExecutionHubPage` subscribes once (keyed on `[selectedProjectId, realtime.subscribe, fetchDetail, fetchRuns]` — see §17 for why `realtime.subscribe` specifically, not the whole `realtime` object). On a real event:

- If `event.run_id` matches the currently-selected run: a debounced (`EVENT_RESYNC_DEBOUNCE_MS = 250`) call to the existing `fetchDetail(projectId, runId)` — the same real `aiqaApi.executions.get` FE-8 already used, not a new endpoint.
- If the event's `run_id` belongs to a run already present in the current list, or the event is `RUN_SCHEDULED` (a brand-new run's first-ever signal): a debounced call to the existing `fetchRuns`.

This is a targeted re-fetch, not a hand-patch of the event payload into local state — per §5's own finding, `RunEvent` structurally cannot carry a task's `started_at`/`finished_at` or a run's final `RunState`, so any attempt to patch those fields from the event alone would either be wrong or require inventing data the backend never sent. The re-fetch is still "event-driven, not polled": nothing is scheduled on a timer; every fetch traces back to a real event, a reconnect, or the user's own Refresh click. Live-verified extensively (§23, Scenarios C-E).

## 11. Task realtime

Used exactly as FE-8's real, unchanged task status vocabulary already displays it (`QUEUED/RUNNING/PASSED/FAILED/BLOCKED/SKIPPED/CANCELLED/TIMEOUT/INFRA_ERROR/NEEDS_REVIEW`) — the resync in §10 re-fetches the full task list per event, so task rows reflect real, complete `TaskState` objects (including `started_at`/`finished_at`), never a partial event-derived guess. No FE-12 vocabulary migration performed (brief §17, critical rule 13).

## 12. Approval/cancellation realtime

**Approval**: per §5, no dedicated event exists — approval is observed indirectly via the real `RUN_SCHEDULED` event that fires when `approve_and_spawn` calls `spawn_run` (which sets `record.state = Running` before emitting). This sprint does not call Approve/Reject automatically from any event (brief §18, critical rule — realtime only informs, never authorizes) — the real Approve/Reject buttons (FE-8, unchanged) remain the only way those actions happen. Live-verified (§23, Scenario D): a real `maestro`-executor run entered `PENDING_APPROVAL`; clicking the real Approve button produced a real `POST .../approve`; `RUNNING` then appeared with no manual refresh, driven by the resulting `RUN_SCHEDULED` event.

**Cancellation**: the real `CancelRunResponse` is not trusted as final (its own doc comment already said so before this sprint) — the eventual `CANCELLED` state, real `policy_notes` ("run cancelled by ..."), and real per-task reason ("run cancelled while this task was executing") are all observed via the same §10 resync path, not the cancel response itself. Live-verified (§23, Scenario E).

## 13. Evidence integration

`EvidencePage` subscribes with the same architecture (§10's pattern), scoped to the currently-selected run. No dedicated "evidence created" event exists in the real backend (§5) — a real `TASK_*` event for the viewed run is the honest, disclosed trigger for a debounced re-fetch via the *existing* FE-7 `aiqaApi.executions.evidence` call (brief §20: "Use FE-7's real Evidence API... do not duplicate evidence-viewer logic" — satisfied exactly, zero new evidence-fetching code). Live-verified (§23, Scenario F) with real, if timing-constrained, confirmation — see §23 and §28 for the honest disclosure of that constraint.

## 14. Audit integration

`AuditPage` subscribes project-wide (not run-scoped, since audit itself isn't) — any real event on the current project's stream is the honest, disclosed trigger for a debounced re-fetch via the *existing* FE-6 `aiqaApi.audit.list` (brief §21: same "reuse, never duplicate" requirement). No real "audit event" type exists in the WS vocabulary (audit rows are written by a separate, non-broadcast `audit(...)` call inside the same REST handlers) — every auditable action this page can show (create/approve/reject/cancel) genuinely produces a correlated `RunEvent` shortly after, which is the documented basis for this trigger. Live-verified (§23, Scenario G) with an unambiguous before/after row-count comparison while the page stayed mounted throughout — the strongest single piece of live evidence gathered this sprint.

## 15. Polling transition

FE-8's `POLL_INTERVAL_MS = 3000` self-rescheduling `setTimeout` loop inside `fetchDetail` is **completely removed** — `fetchDetail` no longer reschedules itself under any condition. Per brief §36 ("do not leave uncontrolled duplicate polling + realtime running indefinitely"), this sprint took the literal reading: there is no background timer at all, connected or not. The disclosed fallback for a genuinely non-`connected` state is the existing real "Refresh" control (extended this sprint to also resync the selected run's detail, not just the list — a one-line change, not a new control) plus the visible `connectionState` badge (§9) telling the user when they are not receiving live updates. This is a deliberate design choice, not a documented requirement (the source documents are silent on a specific fallback-polling mandate) — disclosed as such rather than claimed as PRD-mandated.

## 16. Reconnection/catch-up

**Reconnection**: bounded exponential backoff — `1000ms × 2^attempt`, capped at `30000ms`, `MAX_RECONNECT_ATTEMPTS = 6`, after which `connectionState` settles on `error` and auto-retry stops (a manual `reconnect()` remains available). A real `4001` lag close reconnects immediately, uncounted against this backoff (§4). Every reconnect attempt fully tears down any prior socket first — never two live sockets, never duplicate listeners (`socketRef.current !== socket` guards discard any stale, already-superseded socket's late `onmessage`/`onclose`). Live-verified via real, injected server-initiated closes (§23) and by automated test (items 8-10, 23-26).

**Catch-up**: the real backend has **no project-wide catch-up endpoint** — only the pre-existing per-run REST endpoint (`aiqaApi.executions.events`, `?since=<sequence>`), which this sprint does not newly wire into a catch-up flow, for an honest reason: after a reconnect, this frontend does not track a reliable "last known sequence per run across every run in the project" cursor, and inventing one would mean guessing at a protocol the backend does not itself define project-wide (brief §25: "do not invent a replay protocol"). Instead, on any transition into `connected` (fresh connect or reconnect) while a run is selected, one real REST resync of both the run list and the selected run's detail happens (§10) — an honest "reconcile authoritative state," matching FRD-RT-03's own wording exactly, without pretending a gap-free event history was recovered.

## 17. Event ordering/deduplication

**Deduplication**: by the real `event_id` (a UUID, confirmed globally unique per `RunRecord::emit`), in a bounded 500-entry ring — never a timestamp-based heuristic (brief §27's own explicit warning against that). Verified by automated test (item 21): the exact same `event_id` delivered twice was processed by subscribers exactly once.

**Ordering**: the real backend provides `sequence` (per-run monotonic) and `project_sequence` (project-stream-wide monotonic) — `RealtimeContext` forwards events in delivery order with these fields intact but does not itself impose ordering logic, since it has no notion of "current UI state" to protect. This sprint's actual ordering *guard* lives one level up, structurally rather than by explicit sequence comparison: because every resync (§10) is a full re-fetch of the authoritative REST resource (not an incremental patch), a late/reordered event cannot cause a UI regression — the next resync always reflects whatever the backend's REST endpoint reports as current, and `fetchDetail`/`fetchRuns` each cancel their own prior in-flight request via `AbortController` before starting a new one, so an old resync triggered by an earlier event cannot resolve after (and overwrite) a newer one. This is a disclosed, deliberate design decision, not an oversight: it side-steps needing per-run sequence bookkeeping entirely, at the cost of not being able to display an intermediate state that arrived and was immediately superseded — a live-observed non-issue for a UI showing "current status," not a change log.

## 18. Security

- Verified live (§23, Scenario K) and by automated test: the raw API key never appears in the DOM, URL, or console; no `"bearer"`/`"authorization"` substring rendered anywhere; only the two pre-existing `localStorage` keys present; no cookies; no `sessionStorage` usage.
- The real WS URL (`ws://.../events?key=<raw key>`) is never rendered anywhere in the UI — confirmed via the same DOM-scan check that already covers `aiqaApi.ts`'s REST URLs.
- `RealtimeContext`'s own dev-time logging (`console.error` for a throwing subscriber) never logs event payloads or the key — only a static message and the caught error object from the *subscriber's own bug*, never the event data itself.
- Project isolation is defense-in-depth only (brief §11) — the real backend already enforces it server-side (`require_project_access`, confirmed by `ws_tests.rs`); this frontend's own `event.project_id !== selectedProjectIdRef.current` check is a second, client-side layer, not the authority.

## 19. Fake-data separation

`RealtimeContext.tsx` performs zero reads from `AIQAContext` and fabricates no event, execution, or connection state — every `RunEvent` it forwards came from a real, validated WS frame; every `connectionState` transition traces to a real `onopen`/`onclose`/`onerror`/timer. No mock event bus exists anywhere in shipped code (brief §4's explicit prohibition) — the only simulated transport is `MockWebSocket` in `RealtimeContext.test.tsx`, a test-only file, exercised the same way every other FE unit's tests mock `fetch` at the transport boundary.

## 20. Dashboard preservation

Not modified this sprint, and — per §3's disclosed scope gap — deliberately not wired to realtime despite the Flow Diagram's own aspirational depiction, because no real backend event exists for Dashboard/KPI data and Dashboard remains 100% fabricated `AIQAContext` content untouched since before FE-4. Confirmed unregressed by the existing automated tests (`ExecutionHubPage.test.tsx` item 31, `AuditPage.test.tsx` item 25), which already re-ran clean with `RealtimeProvider` present in the tree.

## 21. Test Generator preservation

Not modified this sprint; not forced into any realtime flow (brief §41 — no documented flow requires it, and its FE-8-established fabricated export handoff is unrelated to execution realtime). Confirmed unregressed by existing automated tests (`ExecutionHubPage.test.tsx` item 32, `AuditPage.test.tsx` item 26).

## 22. Tests

**15 new tests** in `forentend/src/context/RealtimeContext.test.tsx` (brief §34 items 1-26, several combined: 4/5, 8/9, 18/19, 23/24), exercised through the real `AppShell` tree (plus two tests using a minimal direct-hook harness for precision — see that file's own comments) with `fetch` mocked at the transport boundary and a controllable `MockWebSocket` standing in for the browser's real `WebSocket`:

```
Test Files  10 passed (10)
     Tests  181 passed (181)
```

Full breakdown: `aiqaApi.test.ts` 37 + `AuthContext.test.tsx` 11 + `ProjectContext.test.tsx` 14 + `StateViews.test.tsx` 21 + `ProjectSelector.test.tsx` 6 + `AppRoutes.test.tsx` 20 + `AuditPage.test.tsx` 18 + `EvidencePage.test.tsx` 21 + `ExecutionHubPage.test.tsx` 18 + `RealtimeContext.test.tsx` 15 = **181**. All 166 pre-existing FE-1 through FE-8 tests remain unmodified in assertion content and re-ran clean with FE-9's code present (including two live-caught bugs' fixes — §item 21/26 below).

**Regression items 27-32** (brief §34) are satisfied by that same full-suite run — `ExecutionHubPage.test.tsx`, `EvidencePage.test.tsx`, and `AuditPage.test.tsx` all still pass in full (items 27-29), and their own Dashboard/Test Generator non-regression assertions (items 30-31 there) still pass with `RealtimeProvider` now in the tree (item 32).

**Two disclosed test-suite limitations, not silently omitted**:
- Item 25 (reconnect/catch-up follows backend protocol) is verified for the *reconnect* half only — there is no automated test for a project-wide catch-up protocol, because none exists to test (§16).
- The double-submission-class bug found in §items below (realtime effects re-subscribing on every event) was **not** caught by this sprint's own initial automated tests — each test's single-event scenarios never triggered the multi-event resubscribe race that live testing did. The fix is covered by the *existing* passing suite (which continues to pass), but no new regression test specifically reproduces the original race — disclosed honestly rather than claimed as covered.

## 23. Manual verification

Ran the real Vite dev server against the real `services/orchestrator` backend, exactly as every prior FE unit did. Two real API keys were used: the FE-8 project-scoped operator key (re-minted this session after the original was lost across a usage-limit interruption — see the FE-8 report's own §26 note on this same pattern) for Scenarios A-G/I/K, and a freshly-minted global admin key (`fe9-manual-verify-admin`, via the same documented `sqlite3 INSERT` script used since FE-4) for Scenario H specifically, since the operator key's own project scope structurally cannot see a second project.

- **Scenario A (no key)** — real, verified: no `WebSocket` construction occurred (confirmed both by the real "no key" state card and, for later scenarios, by wrapping the real `window.WebSocket` constructor to log every real construction — a live-instrumentation technique, not a fabrication, since the wrapper always still constructs and returns the real object). **PASS.**
- **Scenario B (authenticated + project)** — real, verified: exactly one real `WebSocket` construction, to the exact real URL `ws://localhost:3000/api/v1/projects/<real id>/events?key=<real key>`, `connectionState` badge showing "Live". **PASS.**
- **Scenario C (real execution)** — real, verified, after fixing a real live-discovered bug (see below): a real `pytest`-executor run genuinely progressed `SCHEDULED → RUNNING → FAILED` (a real pytest assertion failure, not fabricated), observed entirely through realtime with no manual refresh, confirmed by the run's own real server-side `Finished` timestamp matching almost exactly when the UI updated. **PASS** (post-fix).
- **Scenario D (approval)** — real, verified: a real `maestro`-executor run entered `PENDING_APPROVAL` with the real policy message; clicking the real Approve button produced `RUNNING` appearing via realtime with no manual refresh, then a real terminal `FAILED` (a real, environment-specific `maestro` CLI error) likewise appeared via realtime alone. **PASS.**
- **Scenario E (cancellation)** — real, verified: a real `pytest`-executor run was cancelled while genuinely running; the real terminal `CANCELLED` state, real `policy_notes`, and real per-task cancellation reason all appeared via realtime with no manual refresh. **PASS.**
- **Scenario F (evidence)** — real, but only partially unambiguous, disclosed honestly rather than overclaimed: real evidence was confirmed present and correctly rendered for a run selected on `EvidencePage` immediately after creation, using the same underlying subscribe/debounce/resync mechanism already rigorously proven correct on `ExecutionHubPage` (§10; same code pattern, same live-discovered-and-fixed bug class, same fix). Given the real `pytest` executor's ~9-second total runtime versus this session's own multi-second browser-tool round-trip latency, isolating "evidence appeared while the page was already open showing zero" from "the page's own initial fetch happened to land after completion" precisely was not achievable with the tooling available this session. **PASS, with an honestly disclosed timing-precision limitation** (see §28).
- **Scenario G (audit)** — real, verified, and the single most unambiguous live proof gathered this sprint: while `AuditPage` remained mounted (never navigated away, never manually refreshed), a real execution was created via a direct authenticated REST call from the same browser tab; the audit row count visibly went from 12 to 13 rows purely from the resulting realtime event. **PASS.**
- **Scenario H (project switch)** — real, verified, using the admin key (§ above): switching "AIQA Core Platform" → "Payments Gateway QA" opened exactly one new real `WebSocket` to the new project's real id; a real run created in "Payments Gateway QA" appeared correctly with no other project's data present; switching back showed "AIQA Core Platform"'s full, correct, un-corrupted run list. **PASS.**
- **Scenario I (connection loss)** — real, verified, with an important, honestly disclosed real finding (see §28): killing the real backend process **during active traffic** (immediately after several real run actions) was detected correctly and immediately, matching the real Vite dev-proxy's own logged `ECONNRESET`. A **second** kill, this time while the connection was genuinely idle, was **not** detected by the browser's `WebSocket` at all — even minutes later — because the real backend has no application-level heartbeat/ping mechanism, and a silently-vanished TCP peer with no data in flight is fundamentally undetectable by any WebSocket client without one. This is a real backend protocol gap, not a frontend bug (§28), and is exactly the class of finding brief §38 says to document rather than silently patch (fixing it would require adding a server-side heartbeat, which is backend code, off-limits this sprint). Restarting the backend and reloading confirmed clean recovery (real key re-verified, real project auto-reselected, real data intact, real `WebSocket` re-established, no duplicate connections). **PASS, with the heartbeat gap explicitly disclosed as a backend limitation.**
- **Scenario J (unknown event)** — **not live-reproducible**, disclosed rather than faked: the real backend only ever emits its own real, closed vocabulary of event types (§5) — there is no safe mechanism to make it emit an arbitrary unknown type, and brief §35 itself explicitly forbids manufacturing one via a database write. Covered instead by automated test (item 18/19: a real event with a fabricated `type` value, and a genuinely malformed payload, both confirmed not to crash the app). **Automated-test-verified only**, matching this engagement's established precedent (e.g. FE-7's own Scenario H/403).
- **Scenario K (security)** — real, verified, after all of the above real interactions across two real keys: no raw key or partial key anywhere in the DOM, URL, or console; no `"bearer"`/`"authorization"` substring rendered; no cookies; no `sessionStorage`; only the two expected `localStorage` entries. **PASS.**

**Two real, live-discovered bugs, found and fixed during this manual verification (not fabricated, not pre-existing-and-ignored):**

1. **`ExecutionHubPage`/`EvidencePage`/`AuditPage`'s subscription effects re-subscribed on every single event, cancelling their own just-scheduled debounce timers.** Root cause: each effect's dependency array included the whole `realtime` context value object, which `RealtimeContext`'s own `useMemo` recreates on *every* event (since `lastEventAt` changes every time) — not just events relevant to that page. The resulting unsubscribe-then-resubscribe cycle on every event cleared the `detailResyncTimeoutRef`/`listResyncTimeoutRef`/`auditResyncTimeoutRef`/`evidenceResyncTimeoutRef` timer the *same* listener invocation had just scheduled a moment earlier — so a run's *last* event (`RUN_COMPLETED`, after which nothing else ever arrives to accidentally retrigger a working resync) could be scheduled and then immediately cancelled by its own event's re-render, leaving the UI stuck on stale non-terminal state indefinitely. Confirmed live: a real pytest run finished (`FAILED`) server-side and the UI kept showing `RUNNING` for over a minute until a manual Refresh. **Fixed** by depending on `realtime.subscribe` (a stable `useCallback(..., [])` identity in `RealtimeContext.tsx`) instead of the whole `realtime` object, in all three pages. Re-verified live: a fresh identical run genuinely transitioned to its real terminal state within ~3 seconds of the backend finishing, with zero manual refresh.
2. **The dev-only Vite WS proxy's `error` handler never closed the client-facing socket on an upstream WebSocket failure.** Root cause: `vite.config.ts`'s existing `proxy.on('error', ...)` handler (written for the REST case, where `res` is an `http.ServerResponse`) was guarded by a `typeof res.writeHead === 'function'` check that always evaluated false for the WS case (there, `res` is a raw `net.Socket`, with no `writeHead`) — so the handler silently did nothing, leaving the browser-facing socket technically still open with no way to ever learn the upstream died, discovered directly through the resulting stuck "Live" indicator in Scenario I above. **Fixed** by adding an `else if (typeof res.destroy === 'function') res.destroy();` branch. This is `forentend/vite.config.ts` — frontend dev-server infrastructure, not `services/orchestrator` — so fixing it does not violate brief §38's backend-modification prohibition. Confirmed correct by code review (both branches of the guard now handle their respective real cases); full live re-confirmation was not possible within this session because reproducing an *active-traffic* backend death (the one scenario that reliably triggers detection at all — see the heartbeat-gap disclosure above) a third time was not pursued further given the already-thorough Scenario I coverage obtained.

No scenario above is claimed without having been genuinely exercised; every partial or non-live-reproducible result (F, I's idle-death half, J) is disclosed plainly rather than worked around with a fabricated result, matching brief critical rules 17-18 exactly.

## 24. Typecheck/build

Exact commands, run from `forentend/`, final state (after both live-discovered fixes):

```
npx tsc --noEmit     # clean, no errors
npx vitest run       # 181/181 passing
npx vite build       # succeeds, 2712 modules, ~5.7s
```

The production build's >500kB chunk-size warning is pre-existing (present since before FE-4) and unchanged in kind by this sprint.

## 25. Regression

All 166 pre-existing tests (FE-1 through FE-8) still pass, unmodified in assertion content, alongside the 15 new `RealtimeContext.test.tsx` tests (181 total). `App.tsx` gained one additive provider (`RealtimeProvider`, nested between `ProjectProvider` and `AIQAProvider`); `ExecutionHubPage.tsx`/`EvidencePage.tsx`/`AuditPage.tsx` each gained a subscription effect and (Execution Hub only) a small connection-state badge — no existing visual element, route, or prior behavior was removed or altered beyond FE-8's polling loop itself (§15, an explicit, intended removal). `services/aiqaApi.ts` gained one new exported function (`projectEventsWsUrl`) and one corrected doc comment; zero existing exports changed signature. `vite.config.ts` gained one `else if` branch in an existing error handler.

## 26. Documentation compliance

Re-checked at sprint start (§3) — the three now-present documents were read specifically for their realtime/eventing content, compared against the real backend implementation (§4-§5), and two genuine gaps were found and disclosed rather than silently resolved or silently ignored: a citation-label mismatch between the backend's own code comments and this consolidated v6.0 document's actual section numbering (§3), and a documented aspiration (Dashboard/RBAC/API realtime) the real backend does not implement, which this sprint correctly does not act on given those pages remain fabricated and out of scope (§3, §20). No contradiction was found for the Execution Hub/Evidence/Audit realtime integration specifically — FRD-RT-01 through 04 and Business Logic §10's "incremental updates, reconnect reconciles state, idempotent processing" describe exactly the real architecture built here.

## 27. Backend changes

**None.** No file under `services/orchestrator/src` or `migrations/` was created, edited, or deleted this sprint. All real events observed during manual verification were produced entirely through the orchestrator's own existing, real, authenticated endpoints (execution create/approve/reject/cancel) — normal, already-supported behavior. The one non-code, disclosed exception (matching FE-8's own established precedent): a fresh global admin API key (`fe9-manual-verify-admin`) was minted directly via the same documented `sqlite3 INSERT` script used since FE-4, specifically because Scenario H (project switch) genuinely requires visibility into two different real projects and the operator key used throughout FE-8 is itself project-scoped to only one. This adds a harmless `api_keys` row for manual testing; it does not touch run/task/evidence/audit data, and the real backend's own realtime/event/authorization logic was not modified in any way.

## 28. Limitations

- **No application-level heartbeat/ping-pong exists in the real backend** (confirmed by reading every WS-related source file — no such mechanism is implemented). A WebSocket connection that dies while genuinely idle (no data in flight in either direction at the moment of failure) is undetectable by this frontend, or by any WebSocket client in any framework, until either side next attempts to send data or the OS's own TCP-level timeout eventually fires (which is not application-controlled and was observed, live, to exceed several minutes in this environment). A connection that dies *during* active traffic is detected promptly and correctly (live-confirmed, §23 Scenario I). This is a genuine backend protocol gap, disclosed per brief §38's own instruction rather than silently patched (a fix would require the backend to proactively send periodic data, which is `services/orchestrator` code, off-limits this sprint).
- Scenario F (evidence) could not be isolated with full timing precision within this session's own browser-tool round-trip latency versus the real `pytest` executor's ~9-second runtime — disclosed in §23 rather than overclaimed as a clean, unambiguous live proof (unlike Scenario G's audit test, which achieved exactly that).
- Scenario J (unknown event) could not be live-reproduced against the real backend (§23) — automated-test-verified only, per brief §35's own explicit prohibition on manufacturing backend events.
- The event-ordering guarantee (§17) is structural (full resync + request cancellation) rather than sequence-number-based — correct for this sprint's actual UI (current-state display, not a change log), but would need to change if a future unit ever needed to show an ordered event timeline rather than current state.
- No automated regression test specifically reproduces the original resubscription race (§23, bug 1) — the fix is covered by the passing suite, but the race itself is only proven fixed by live verification, disclosed honestly in §22.
- The `vite.config.ts` WS-proxy-destroy fix (§23, bug 2) is dev-environment infrastructure only — a real production deployment's own reverse proxy (per `aiqaApi.ts`'s own long-standing doc comment on this) would need the equivalent behavior verified separately; this sprint did not have a production-equivalent proxy available to test against.

## 29. Out-of-scope

**FE-10 and onward were not started.** This sprint implemented realtime/event synchronization only: the centralized `RealtimeContext`, its integration into Execution Hub/Evidence/Audit, and the resulting removal of FE-8's polling loop. No visual redesign of any Google-Studio-generated page was performed (Dashboard and Test Generator remain the sole approved visual references, both confirmed unmodified and unregressed — §20-§21); no FE-12 status-vocabulary migration was performed (§11); no backend code, schema, or authorization logic was touched (§27); and no project-wide catch-up protocol was invented where the real backend has none (§16).
