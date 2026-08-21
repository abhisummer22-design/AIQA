# AIQA — Phase 2 — BE-14 Implementation Report

**Unit:** BE-14 — Project-Scoped Realtime Event Stream (Integration Plan milestone table: `BE-14 | Project event stream | §14 with per-project sequence + resync-on-lag`)
**Mode:** Implementation sprint (no large independent verification pass, per instruction)
**Status: COMPLETE**

## 1. Architecture decision (Step 2)

**Transport:** WebSocket, reusing the existing per-run socket's proven `?key=` query-parameter auth technique — not a new mechanism. Basis: Integration Plan §3.2 ("`?key=` query-param handshake exists because browsers cannot set headers on a WS upgrade — a universal constraint that will apply to the contract's stream too. Keep the technique; re-target the URL.") and §11.2 item 1 ("Add project-scoped stream `/api/v1/projects/{id}/events`, multiplexing that project's runs. Keep the per-run socket internally.").

**G-WS is explicitly a *deferred* contract ambiguity, not something BE-14 resolves.** The Integration Plan states G-WS "must be resolved in the source documents before realtime... work starts. This plan does not resolve them" (§6.1) and separately lists three long-term options — query param / short-lived ticket / cookie — as "to be decided before Phase 3... This plan does not choose" (§9.4). BE-14 reuses today's working query-param technique (option (a), already in production) rather than picking a different one, exactly as §3.2/§11.2 instruct. The deeper "should this eventually change" question remains open — see §13.

**Event source:** the single existing event-creation path, `RunRecord::emit()`. No new event-creation code, no timer, no polling. Every call to `emit()` now additionally fans the same `RunEvent` out to that run's project's realtime channel.

**Authorization:** the exact same helpers every REST scoped route uses — `state.store.get_project()` (existence) then `auth::require_project_access()` (ownership) — no new authorization logic.

**Event envelope:** additive fields only (`project_id`, `project_sequence`) on the existing `RunEvent`; internal event-type vocabulary (`TASK_PASSED`, `RUN_COMPLETED`, …) is unchanged — mapping it to the contract's vocabulary (`TASK_STATUS_CHANGED`, `MODEL_*`, …) is explicitly out of scope: Integration Plan §11.2 item 2 states that mapping "is a §31 decision, not an implementation choice."

## 2. Scope boundary honored

Implemented: project-level subscription route, subscription auth, project authorization, project-filtered real event delivery, minimal envelope metadata, disconnect handling, targeted tests, and the Lagged/resync fix explicitly listed as part of this same Realtime Strategy item set (§11.2 item 5).

Not implemented (explicitly out of scope, not silently solved): G-WS's long-term auth-mechanism redesign; G-EVENT vocabulary remapping (§31 decision); G-AUTHN; any user/session model; any frontend code; BE-15; denormalizing `project_id` onto the `run_events` table (not required — see §5).

## 3. Endpoint

`GET /api/v1/projects/{project_id}/events?key={api_key}` — WebSocket upgrade. Mounted outside the header-based `require_auth` layer, alongside `/runs/:id/ws`, for the same reason: a WS upgrade cannot carry an `Authorization` header.

Rejections (pre-upgrade, plain status + text — deliberately not the JSON envelope, matching the existing, previously-documented decision for `/runs/:id/ws`; see `errors.rs`'s module doc comment / G-WS):
- No/invalid `?key=` → `401`
- Project does not exist → `404`
- Caller's key is scoped to a different project → `403`

## 4. Real event fan-out (no fake data)

- `AppState` gained `project_streams: HashMap<String, Arc<ProjectStream>>` (lazily created per project) and `project_stream(project_id) -> Arc<ProjectStream>`.
- `ProjectStream` = one `broadcast::Sender<RunEvent>` (capacity 1024, matching the existing per-run channel) + one `AtomicU64` sequence counter.
- `create_run_core` resolves the run's project stream once and hands it to `RunRecord::new`.
- `RunRecord::emit()` — the one place every real event is created — now does, after its existing per-run persist/broadcast: if this run has a project stream, clone the same event, stamp it with `project_id` and the *project's own* next sequence number, and publish it there too. Nothing is invented; it is the same `RunEvent`, published a second time to a second, project-scoped audience.
- A run with no project (`project_id: None`) has no project stream and simply never appears on any project's feed — correct, not a gap.

## 5. Event envelope

`RunEvent` gained two additive fields (`#[serde(default)]`, so nothing that deserializes the old shape breaks):
- `project_id: Option<String>` — real, from `RunRecord.project_id`. `None` for an untenanted run and for an event reconstructed from `run_events` via `list_events_since` (that table has no `project_id` column — deliberately not added; see below).
- `project_sequence: Option<u64>` — real, project-wide monotonic counter, **only set on the copy published to the project stream**. The run-local `sequence` field is completely unchanged and still what `?since=` catch-up uses — this is the "subtlest item in the plan" (§11.2 item 4): two concurrent runs' own per-run sequences both start at 0 and would collide as a merge key if multiplexed together, so the project stream needs its own, separate counter, which it now has.

**No migration.** `project_id`/`project_sequence` are populated at emit-time from already-known in-memory state, not persisted columns. Denormalizing `project_id` onto `run_events` was considered and rejected as unnecessary for this unit: the live path (which is what a realtime *stream* is for) gets it for free from `RunRecord`; only the DB-reload fallback (a run not held in this process's memory) would benefit, and that path honestly reports `project_id: None`/`project_sequence: None` rather than adding schema for a fallback case BE-14 doesn't require.

## 6. Ordering and delivery semantics (Step 7 — honest, not invented)

- Events persisted before subscription remain available through the existing per-run catch-up endpoint (`GET .../executions/:run_id/events?since=`), unchanged.
- The project stream delivers events occurring **after** a successful subscription only — there is no backlog replay on this endpoint (unlike the per-run socket, replaying "every event from every run this project has ever had" on connect is not something to hold in memory or dump at once).
- Ordering: `project_sequence` is monotonic per project stream, so a client can merge/dedupe by it exactly the way the existing frontend algorithm already merges by per-run `sequence` (Integration Plan §3.1).
- Delivery is **at-most-once with detectable loss**, not exactly-once: `tokio::broadcast` can drop old messages for a slow consumer. This was already true of the existing per-run socket; the difference is it is now surfaced (§7 below) instead of silently swallowed.
- Reconnect behavior itself (client-side re-fetch/backoff) is not invented here — out of scope (frontend, §11.2 items 6-8).

## 7. Lagged-path fix (§11.2 item 5 / audit M5)

Previously: `Err(RecvError::Lagged(_)) => continue` — a slow subscriber silently missed events with zero signal. Fixed identically on **both** the per-run and the new project socket (they now share one consumption loop, `drive_socket`, so the fix isn't duplicated code): on `Lagged`, the connection is closed with a distinct WebSocket close code (`4001`) and a reason string naming how many events were skipped, telling the client unambiguously to reconnect and re-catch-up via REST rather than trusting a partial merge. No fabricated "resync marker" event was invented in-band — closing is the honest signal.

## 8. Disconnect and resource safety (Step 8)

- `drive_socket` exits on the client closing (`Message::Close`/socket EOF/error) or on `Lagged`, in all cases dropping its `broadcast::Receiver`, which automatically decrements the channel's subscriber count — verified directly in `be14_disconnect_releases_the_subscriber` via the real `broadcast::Sender::receiver_count()`, not inferred from "no crash happened."
- No unbounded queue was added: the project channel reuses the same fixed capacity (1024) the per-run channel already uses, with the same backpressure policy (drop-oldest + `Lagged` signal, now handled rather than swallowed).
- No new spawned task per subscriber beyond the one `ws.on_upgrade` already spawns (unchanged shape from the existing per-run handler).

## 9. Files changed

- `services/orchestrator/src/domain.rs` — `RunEvent` gained `project_id`/`project_sequence` (additive).
- `services/orchestrator/src/engine.rs` — new `ProjectStream` type; `RunRecord` gained a `project_stream` field + constructor parameter; `emit()` fans out to it with its own sequence.
- `services/orchestrator/src/state.rs` — `AppState.project_streams` registry + `project_stream()` getter (get-or-create).
- `services/orchestrator/src/store.rs` — `list_events_since`'s reconstructed `RunEvent`s set the two new fields to `None` (no DB column exists for them, disclosed rather than guessed).
- `services/orchestrator/src/api.rs` — new `project_events_ws_handler`; `handle_socket`/the per-run handler and the new one now share one `drive_socket` loop (Lagged fix applied once, reused); new route mounted alongside `/runs/:id/ws`; `create_run_core` resolves and passes the project stream.
- `services/orchestrator/src/lib.rs` — registers the new `ws_tests` module.
- `services/orchestrator/Cargo.toml` — `tokio-tungstenite`/`futures-util` added as **dev-dependencies only**; both were already transitive dependencies of `axum`'s `ws` feature (confirmed via `Cargo.lock` before adding), so this exposes existing infrastructure for tests rather than introducing anything new.
- `services/orchestrator/src/engine_tests.rs` — mechanical update: all 15 pre-existing `RunRecord::new(...)` call sites pass the new trailing parameter as `None` (no project stream needed for those tests' own concerns).
- `services/orchestrator/src/ws_tests.rs` — new, 9 tests.

## 10. Migrations added

None.

## 11. Tests added (9, in `ws_tests.rs`)

Unlike `api_tests.rs`'s in-process `tower::oneshot` (which cannot drive a WebSocket upgrade), these bind the real router to a real, ephemeral TCP port and connect with a real `tokio-tungstenite` client — a subscription test exercises the actual upgrade/auth/multiplex path, not an internal broadcaster called directly.

1. `be14_authorized_subscriber_can_connect_to_project_stream`
2. `be14_missing_key_is_rejected` — 401
3. `be14_nonexistent_project_is_rejected` — 404
4. `be14_cross_project_subscription_is_rejected` — 403
5. `be14_subscribers_receive_only_their_own_projects_real_events` — two real projects, real `POST .../executions` triggers via the real router (not `RunRecord::emit` called directly); each subscriber gets only its own project's events; explicit cross-project silence check in both directions
6. `be14_disconnect_releases_the_subscriber` — verified against the real `broadcast::Sender::receiver_count()`
7. `be14_per_run_events_rest_endpoint_still_works` — regression
8. `be14_legacy_per_run_websocket_still_works` — regression
9. `be14_lagging_subscriber_is_closed_with_distinct_code_not_silently_skipped` — deterministic (non-yielding flood past channel capacity), not timing-dependent on real network backpressure

Not added: a "call the broadcaster directly and assert it broadcasts" test — that would be exactly the shortcut the brief prohibits ("do not merely call an internal broadcaster directly in a way production code never uses"); every event-receiving test instead triggers a real `POST .../executions`.

## 12. Targeted test result

```
running 9 tests
test ws_tests::be14_missing_key_is_rejected ... ok
test ws_tests::be14_nonexistent_project_is_rejected ... ok
test ws_tests::be14_cross_project_subscription_is_rejected ... ok
test ws_tests::be14_authorized_subscriber_can_connect_to_project_stream ... ok
test ws_tests::be14_lagging_subscriber_is_closed_with_distinct_code_not_silently_skipped ... ok
test ws_tests::be14_legacy_per_run_websocket_still_works ... ok
test ws_tests::be14_disconnect_releases_the_subscriber ... ok
test ws_tests::be14_per_run_events_rest_endpoint_still_works ... ok
test ws_tests::be14_subscribers_receive_only_their_own_projects_real_events ... ok

test result: ok. 9 passed; 0 failed
```

## 13. `cargo check --all-targets`

Clean — no warnings, no errors.

## 14. Full `cargo test` result

```
test result: ok. 88 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in ~2.9s
```
79 pre-existing (D1–D5, B-series, BE-8, BE-9 errors, BE-10, Actual BE-9, BE-12, BE-13) + 9 new BE-14 tests. Re-ran the full suite 4 times consecutively to rule out flakiness from the real-TCP/real-WebSocket tests — 88/88 every run. No existing test's *assertion* changed; the only change to `engine_tests.rs` was the mechanical trailing `None` argument required by `RunRecord::new`'s new (optional-value) parameter.

## 15. Explicit out-of-scope work untouched

BE-15, G-AUTHN, broad G-WS redesign (the long-term auth-mechanism decision — query param vs ticket vs cookie — remains open, per §9.4), G-EVENT vocabulary remapping (§31 decision), frontend code (`forentend/`, `apps/web/`), `/metrics`, D1–D5/B-series guarantees, BE-12 dashboard/KPI behavior, BE-13 cancellation behavior and real subprocess interruption, structured REST errors, legacy `/runs/...` routes (all left fully functional and regression-tested), `AIQA_API_CONTRACT_v1.0.md`/`AIQA_ACCEPTANCE_MATRIX_v1.0.md` (not modified).

## 16. Remaining blocker or limitation

None blocking. Disclosed, non-blocking limitations:
- The project stream has no on-connect backlog replay (§6) — by design, not an oversight; a client wanting history uses the existing per-run `?since=` endpoint.
- `project_id`/`project_sequence` are `None` on events reconstructed from the DB fallback path (a run not held in this process's memory) — an honest gap, not a fabricated value, and not a path a live realtime stream needs.
- G-WS's deeper question (should query-param auth eventually be replaced by a ticket or cookie mechanism) is still open, exactly as the Integration Plan says it must be resolved in the source documents, not silently decided by an implementation unit.
