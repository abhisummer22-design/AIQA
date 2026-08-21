# AIQA — Phase 4 — FE-1 Implementation Report

**Unit:** FE-1 — Frontend Integration Layer (Integration Plan §16: `FE-1 | Integration layer exists | Real client in services/aiqaApi.ts — first fetch in the codebase`)
**Status: COMPLETE**

## 1. Exit criterion met

`forentend/src/services/aiqaApi.ts` now contains a real `fetch()`-based HTTP client — the first `fetch()` call anywhere in `forentend`. It replaces the previous `AIQAClient` stub, which had **zero** `fetch()`/`axios`/`WebSocket` calls and returned hardcoded values from every method (`getActiveRuns()` → `[]`, `triggerRun()` → a synthetic `run-${Date.now()}` id and `status: 'QUEUED'`, `executeEndpointTest()` → always `PASSED`/200, `checkHealth()` → always `status: 'connected'`). Confirmed before starting that nothing in `forentend` imported the old stub (`grep -rl "aiqaApi\|AIQAClient" src` returned only the file itself) — this sprint's changes have zero blast radius on any currently-rendering screen.

## 2. Sources read

- Integration Plan §1 (Architecture Decision — one contract, one backend, `forentend` target), §2 (Target Frontend Decision — `forentend` locked, `apps/web` reference-only), §3 (Reusable Components — `apiClient.ts` ~85% reusable transport/error core; `services/orchestrator.ts` explicitly NOT reusable, flat paths), §4 (Components to Preserve), §16 (Frontend Milestones), §17 (Integration Milestones), §20 (Final Go/No-Go).
- `apps/web/src/services/apiClient.ts`, `apps/web/src/hooks/useApi.ts`, `apps/web/src/components/common/StateViews.tsx` — read for reusable patterns, not copied (see §3).
- `apps/web/vite.config.ts` — the dev-proxy pattern, adapted (§5).
- Current `forentend/src/services/aiqaApi.ts`, `forentend/vite.config.ts`, `forentend/package.json`, `forentend/.env.example` — actual current state, not assumed.
- The real backend surface this client targets: `services/orchestrator/src/{domain,store,api,errors}.rs` — I authored BE-8 through BE-15 this engagement, so every type/route below is verified against the actual Rust source, not the contract's illustrative examples where the two diverge (documented per-field in `contract.ts`).

## 3. §20 Go/No-Go — addressed, not ignored

The plan's original NO-GO (dated before Phase 1/2 work) gates *wiring real UI screens to real data* on BE-1…BE-10 plus resolving G-AUTHN/G-WS in the source documents. Backend-side, BE-1…BE-15 are now complete (this engagement); G-AUTHN/G-WS remain formally undecided in the source documents (Contract/Acceptance Matrix unmodified — confirmed, matches BE-14's own disclosure). This is not a blocker for FE-1 specifically: the plan's own §20 "What IS cleared to proceed now" table explicitly lists *"Port §3.1 ADOPT components into `forentend` — transport-layer, contract-neutral; wire to nothing yet"* as already unblocked regardless of the G-AUTHN/G-WS gate. FE-1 is exactly that — the client exists and is real; it is wired into zero pages (§8).

## 4. What was built

### `forentend/src/types/contract.ts` (new)
Wire types mirrored 1:1 from the actual Rust structs (`RunState`, `TaskStatus`, `EventType` enums; `Project`, `RunPlan`/`TaskDef`, `RunSummary`/`RunDetail`/`RunListItem`, `CreateExecutionResponse`, `CancelRunResponse`, `TaskState`, `RunEvent`, `EvidenceRow`, `Finding`/`FailureCluster`/`FindingAnalysis`, `DashboardResponse`/`KpisResponse`, `AuditRow`, `ApiErrorEnvelope`) — not from the contract document's illustrative examples, which several real endpoints deliberately diverge from (e.g. `CreateExecutionResponse.status` not `test_case_ids`-based; `TaskStatus` still `INFRA_ERROR` not `INFRASTRUCTURE_ERROR`). Each interface's doc comment names the exact Rust source and, where relevant, which prior BE report explains a divergence. Does **not** touch `forentend/src/types.ts` (the existing, page-consumed vocabulary) — no currently-rendering screen's types changed.

### `forentend/src/services/aiqaApi.ts` (replaced)
- **API key storage**: `getApiKey`/`setApiKey`/`clearApiKey`/`hasApiKey`/`maskApiKey`, ported from `apiClient.ts` unchanged (localStorage, never logged, mask-only display).
- **`ApiError`**: ported and *extended* — the original reads plain response text; this one parses Contract §2.5's actual structured envelope (`{error:{code,message,details,request_id}}`), which every REST handler in this backend now returns on failure (BE-9/BE-10), and falls back to raw text only for the one deliberate exception (WebSocket pre-upgrade rejections stay plain-text — see `errors.rs`'s own module doc comment). Exposes `kind`, `status`, `code`, `detail`, `requestId`.
- **Transport core**: `rawRequest`/`requestJson`/`requestVoid`/`requestBlob`, `fetch()`-based, Bearer auth injection, `AbortSignal` support, offline detection (network failure, and 502/503/504 — matching the dev proxy's own 502-on-upstream-down convention, §5).
- **Base path**: `API_BASE = import.meta.env.VITE_ORCHESTRATOR_BASE ?? '/api/v1'` — contract-shaped, same-origin-relative by default (§4 of the brief).
- **Typed endpoint functions**, one per real, implemented route (BE-8…BE-15), grouped under `aiqaApi.{projects,executions,evidence,findings,dashboard,audit}` — every path is the exact `/api/v1/projects/{project_id}/...` route `api.rs` mounts; no legacy flat path (`/runs`, `/bug-drafts`, `/admin/...`) is called. Admin/API-key management is deliberately not covered — that is FE-2's (Auth UI) concern, not the transport layer's.

### `forentend/vite.config.ts` (dev proxy added)
Adapted (not copied) from `apps/web/vite.config.ts`'s pattern: `apps/web`'s proxy strips the `/api` prefix because its client calls legacy flat paths; `forentend`'s client calls `/api/v1/...` directly (the real mount point), so this proxy forwards `/api/v1/*` **unchanged** — stripping it would 404 every real request. `ws: true` is included for BE-14's realtime stream, ready for FE-9 to use later. Same 502-on-upstream-down handler as the reference, so a stopped backend maps to `offline`, not a misleading `server_error`.

### `forentend/.env.example` (documented, not required)
Added commented-out `VITE_ORCHESTRATOR_URL` (dev-proxy target) and `VITE_ORCHESTRATOR_BASE` (browser-side base path) entries, matching the naming convention `apps/web` already established — both optional, both already have working defaults.

## 5. What was deliberately NOT done

- No page/component imports `aiqaApi` or `contract.ts` — zero UI wiring (FE-2 through FE-10's job).
- No hooks ported (`useApiResource`, `usePolledResource`, etc. — §3.1 lists `useApi.ts` as reusable too, but the brief's primary requirement names only `aiqaApi.ts`; porting hooks without anything to wire them to would be scope creep toward FE-3).
- No `StateViews.tsx` six-state UI (FE-3).
- No realtime/`useEventStream` (FE-9) — `aiqaApi.executions.events()` covers only the per-run REST catch-up endpoint, which is real and already implemented; the live WebSocket stream is untouched.
- No admin/API-key management endpoints (key creation/listing/revocation, global audit log) — arguably FE-2 (Auth UI) territory, not transport.
- `forentend/src/types.ts` and every existing page/component: untouched.

## 6. Verification

- `npx tsc --noEmit` (forentend's own `lint` script): clean, no errors.
- `npx vite build`: succeeds (2690 modules, ~9.8s); the pre-existing >500kB chunk-size warning is unrelated (already-known risk R10 in the Integration Plan, not introduced by this change).
- Dev server started for real (`npm run dev`, port 3000) and the app loaded in-browser: renders identically to before (still its existing hardcoded dashboard content, since nothing was wired) — zero console errors, zero server errors. Confirms the `vite.config.ts` proxy addition didn't break app startup.
- Not verified live: an actual authenticated round-trip through the proxy to a running orchestrator (`cargo run` was not started for this check) — not required by FE-1's exit criterion ("first `fetch()` in the codebase" — a transport-layer existence check, not an integration test), and there is no UI path yet that would exercise it. Left for whichever FE milestone first wires a real screen to `aiqaApi`.

## 7. Files changed

- `forentend/src/types/contract.ts` — new.
- `forentend/src/services/aiqaApi.ts` — replaced (fake stub → real client).
- `forentend/vite.config.ts` — dev proxy added.
- `forentend/.env.example` — two new documented (optional) variables.

No backend files touched. No `apps/web/` files touched.

## 8. Explicit out-of-scope work not started

FE-2 (Auth UI), FE-3 (six-state UI), FE-4 (routing), FE-5 (global project scope), FE-6 (audit page), FE-7 (evidence viewer), FE-8 (execution hub real), FE-9 (realtime), FE-10 (dashboard real), FE-11 (fabrication removal), FE-12 (status vocabulary rename), any backend endpoint or refactor, any `apps/web/` change.

## 9. Remaining blocker or limitation

None blocking FE-1 itself. Disclosed for whoever picks up the next FE milestone: G-AUTHN and G-WS remain formally unresolved in the source documents (Contract/Acceptance Matrix have not been modified by any backend unit this engagement) — the client's `?key=`-based auth assumption for a future realtime hook (FE-9) inherits that same, already-documented open question from BE-14, not a new one introduced here.
