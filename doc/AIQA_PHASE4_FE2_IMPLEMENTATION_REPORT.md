# AIQA — Phase 4 — FE-2 Implementation Report

## 1. Status

**PASS**

## 2. Exit criterion

*"Key entry, storage, masked display, 401/403 distinct states"*

- **Key entry**: a real "AIQA Orchestrator API Key" panel, added to `forentend`'s existing Settings → Authentication tab, with a password-type input and Save/Replace action.
- **Storage**: entirely through the FE-1 `aiqaApi.ts` abstraction (`getApiKey`/`setApiKey`/`clearApiKey`/`hasApiKey`/`maskApiKey`) — no component touches `localStorage` directly.
- **Masked display**: `aiqa_r…••••••`-shaped, via `maskApiKey`; the raw value is never re-rendered after save (verified by test and by live browser inspection of the rendered DOM).
- **401/403 distinct states**: a new `AuthContext` classifies `ApiError.kind` into seven distinct `AuthStatus` values, with 401 → `unauthenticated` and 403 → `forbidden` handled as separate, differently-worded, differently-styled UI states (never merged, never auto-retried into each other).

All four satisfied — see §6/§7 for exact mechanics, §11 for live proof.

## 3. Authentication architecture

- **Model**: unchanged from the backend — opaque `aiqa_<hex>` bearer keys, hashed server-side, three fixed roles (viewer/operator/admin), optional project scope. No user/password system, no OAuth, no sessions were introduced.
- **New `forentend/src/context/AuthContext.tsx`**: the single source of truth for frontend auth state. Exposes `status: AuthStatus`, `hasKey`, `maskedKey`, `lastError`, and three actions: `saveKey(raw)`, `validate()`, `signOut()`.
- **`AuthStatus`** (7 values): `no_key` (nothing saved) → `key_present` (saved, not yet confirmed against the backend this session — deliberately distinct from `authenticated`, since a stored key is not proof of validity) → `checking` (request in flight) → one of `authenticated` / `unauthenticated` (401) / `forbidden` (403) / `offline` (network) / `error` (anything else), all derived from the real `ApiError.kind` FE-1 already computes.
- **Validation is explicit, not automatic-on-load**: on mount, a stored key sets `key_present` without firing a network request. The user clicks "Verify Connection" to actually check it. This avoids any unsolicited background retry loop (brief §8: "do not silently retry forever") and keeps the state machine's behavior fully predictable and testable.
- **Wired at the app root**: `App.tsx` now wraps the tree in `<AuthProvider>` (outside the pre-existing `<AIQAProvider>`, which is untouched).
- **UI integration point**: `forentend/src/components/pages/SettingsPage.tsx`'s existing "Authentication & SSO" tab — the brief's own instruction ("if the current UI has a settings page, integrate there"). The new `OrchestratorAuthPanel` component is inserted **above** the tab's pre-existing fabricated SSO/"Rotate Token" content, which is left completely unmodified (§14).

## 4. Storage

The credential lives in `localStorage` under the key `aiqa.orchestrator.apiKey`, exactly as FE-1 already implemented it in `aiqaApi.ts`. `AuthContext` and `SettingsPage.tsx` both go through `getApiKey`/`setApiKey`/`clearApiKey`/`hasApiKey`/`maskApiKey` only — no second storage mechanism, no direct `localStorage.*` call anywhere outside `aiqaApi.ts` itself (confirmed by inspection of every file this sprint touched).

## 5. Security

- Raw key never logged: `grep`ed every new/changed file for `console.log/debug/info` — zero matches.
- Raw key never hardcoded: `grep`ed for an `aiqa_<hex>`-shaped literal across every new/changed file — zero matches.
- Raw key exists only transiently in the input field's local component state (`inputValue`), cleared immediately (`setInputValue('')`) the instant `saveKey` resolves — verified by an automated test (`item 4`) that serializes the entire `AuthContext` value after a save and asserts the raw string is absent.
- The input is `type="password"`, `autoComplete="off"`, `spellCheck={false}`.
- No credential is ever placed in a URL or query string — verified by test (`Bearer credential injection`) asserting the request URL never contains the key.
- Every request carries `Authorization: Bearer <key>` via the existing FE-1 transport core (`rawRequest`) — no new request-building path was added.
- No analytics/telemetry of any kind was added.

## 6. 401 behavior

`ApiError.kind === 'unauthorized'` (HTTP 401) → `AuthStatus = 'unauthenticated'`. UI shows a red banner: *"Authentication required — the orchestrator rejected this key (401). It may be invalid, revoked, or mistyped. Enter a valid key below to continue."* The key input remains available immediately (no extra click needed) so the user can replace it. The key is **not** automatically cleared (brief §12: "do not automatically clear the key unless the existing architecture explicitly requires it" — it does not).

## 7. 403 behavior

`ApiError.kind === 'forbidden'` (HTTP 403) → `AuthStatus = 'forbidden'`. UI shows a distinct orange banner: *"This key is valid but is not permitted to access the requested resource (403) — a role or project-scope restriction, not a bad credential. The key remains configured; no need to re-enter it."* Unlike 401, the key-entry input is **not** surfaced by default (`showInput` stays `false`) — this state does not invite the user to change their credential, matching brief §8 ("Do NOT automatically ask the user to enter another key as though the current key is simply missing").

## 8. Sign-out

`signOut()`: calls `clearApiKey()` (removes the stored credential), resets `status` to `no_key`, clears `lastError`. Touches nothing else — no `AIQAContext` state (projects/runs/settings), no backend call. Verified live in-browser (§11, Scenario D) and by automated test.

## 9. Files changed

- `forentend/src/context/AuthContext.tsx` — new.
- `forentend/src/context/AuthContext.test.tsx` — new, 11 tests.
- `forentend/src/services/aiqaApi.test.ts` — new, 14 tests.
- `forentend/src/components/pages/SettingsPage.tsx` — added `OrchestratorAuthPanel` + its imports; inserted into the existing Authentication tab; pre-existing fake content untouched.
- `forentend/src/App.tsx` — wrapped the tree in `AuthProvider`.
- `forentend/vite.config.ts` — added a Vitest `test` block (`environment: 'jsdom'`, `globals: true`).
- `forentend/package.json` — added `test` script; added `vitest`, `@testing-library/react`, `jsdom` as dev dependencies.

No files under `forentend/src/services/aiqaApi.ts` or `forentend/src/types/contract.ts` (FE-1's own deliverables) were modified — FE-2 only *consumes* them.

## 10. Tests

New test files, 25 tests total, all passing:

**`src/services/aiqaApi.test.ts`** (14):
API key storage — no key by default; store/retrieve; replace leaves only the new one; sign-out clears; masking never contains the raw value. ApiError classification — 401→`unauthorized`; 403→`forbidden`; 401 and 403 are never the same kind; network failure→`offline` never 401/403; a 502 (dev-proxy upstream-down)→`offline`; Contract §2.5 envelope parsing (code/message/request_id). Bearer injection — every request carries `Authorization: Bearer`, never a query-string credential; no key stored → rejects locally with zero network calls. No hardcoded credential in `API_BASE`.

**`src/context/AuthContext.test.tsx`** (11), each mapped to the brief's own minimum list:
1. no key → `no_key` state · 2. save → stored via `aiqaApi` · 3. stored → masked display · 4. raw key absent from the entire context value after save · 5. sign-out clears · 6. replace → only the new key remains · 7. 401 → `unauthenticated`, key stays stored · 8. 403 → `forbidden`, distinct from 401, key stays stored · 9. network failure → `offline`, never 401/403 · plus: `validate()` is a true no-op (zero fetch calls) with no key stored · a full `key_present → checking → authenticated` transition on a successful validation.

```
 Test Files  2 passed (2)
      Tests  25 passed (25)
```

## 11. Manual verification

Ran the real dev server (`npm run dev`, port 3000, proxying to a **real, live `cargo run --release` orchestrator** on port 8080 — not mocked) and drove it via the in-app browser.

- **Scenario A (no key)** — PASS. Settings → Authentication shows "NO KEY CONFIGURED" and a "Save Key" action; no other state is shown as authenticated.
- **Scenario B (save key)** — PASS. Entering and saving a key immediately shows it masked (`aiqa_t…••••••` / `aiqa_r…••••••` depending on the test value used).
- **Scenario C (refresh)** — PASS. After a real full-page reload, the key is still shown as configured (`KEY SAVED · UNVERIFIED`) and masked; the raw value is never revealed.
- **Scenario D (sign out)** — PASS. Clicking "Sign Out" returns the UI to the exact "NO KEY CONFIGURED" state.
- **Scenario E (401)** — PASS, against the real backend. Saved an intentionally-invalid key; the app made a real `GET /api/v1/projects` request through the dev proxy to the live orchestrator, which returned a genuine `401 Unauthorized` (confirmed via the browser's own network log, not asserted from the UI alone). UI showed "UNAUTHENTICATED (401)" and the 401 banner; key remained configured (masked) for editing.
- **Scenario F (403)** — **Not exercised live; documented limitation, not a fabricated pass.** See §16.

**Bug found and fixed during this verification**: the first implementation nested `OrchestratorAuthPanel`'s key-entry `<form>` inside `SettingsPage`'s own pre-existing outer `<form>` (invalid HTML — nested forms have undefined submit behavior across browsers). Clicking "Save Key" was observed live to navigate away to the Dashboard page instead of submitting. Fixed by converting the panel's form to a plain `<div>` with an explicit `onClick`/`onKeyDown(Enter)` handler, re-verified live (Scenarios A–E above are all *post-fix* results).

## 12. Typecheck/lint/build

Exact commands, run from `forentend/`:

```
npx tsc --noEmit     # forentend's own "lint" script — clean, no errors
npx vitest run       # new "test" script — 25/25 passing
npx vite build       # "build" script — succeeds, 2692 modules, ~7.5s
```

The production build's pre-existing >500kB chunk-size warning is unrelated to this sprint (already-known Integration Plan risk R10).

## 13. Backend changes

**None.** `services/orchestrator` was not modified — no source file, no migration, no config. The orchestrator used for manual verification (§11) was the existing `cargo run --release` binary, started and stopped for this verification only; its persistent database (`infra/data/orchestrator.sqlite3`) was not written to by this sprint's own code (only read from, via real HTTP requests the running orchestrator itself served).

## 14. Fake data

Confirmed preserved, not removed:
- `AIQAContext.tsx` (the large fabricated-data provider — projects, runs, RBAC scenarios, API tests, activities, quality intelligence) — **not touched at all**, verified by inspection.
- `SettingsPage.tsx`'s pre-existing fake SSO toggle, IdP dropdown, and "Control Plane Daemon API Token / Status: ACTIVE • Expires in 180 days / Rotate Token" card — **left exactly as they were**; `OrchestratorAuthPanel` was inserted *above* this content, not in place of it. Confirmed live in-browser (§11) that this content still renders unchanged.
- No page was made blank; no existing screen's data was replaced.

## 15. Out-of-scope

Not started: FE-3 (six-state UI / `StateViews`), FE-4 (routing), FE-5 (global project scope), FE-6 (audit page), FE-7 (evidence viewer), FE-8 (execution hub real), FE-9 (realtime), FE-10 (dashboard real), FE-11 (fabrication removal), FE-12 (status vocabulary). No RBAC user-management UI, no OAuth/OIDC, no sessions/refresh tokens, no backend auth redesign, no WebSocket auth redesign. Dashboard/Audit/Evidence/Execution Hub/Findings/Reports remain wired to nothing real (§16 of the brief, honored).

## 16. Limitations

**Scenario F (403) was not exercised live against the real backend.** Root cause: `services/orchestrator`'s persistent database (`infra/data/orchestrator.sqlite3`) already contains API keys from this engagement's prior backend sessions, so the bootstrap-admin-key mechanism (`bootstrap_admin_key_if_needed`, which only fires when zero keys exist) does not re-trigger, and no plaintext key from a prior session was recoverable (keys are stored only as SHA-256 hashes; a repo-wide search for any previously-saved plaintext key found none — correctly, since one should never be committed). Minting a second, differently-scoped key would have required either (a) deleting existing keys to force bootstrap — destructive to persistent state, or (b) writing directly to the database outside the backend's own API — contrary to "backend must not change" / "do not modify the database" in spirit, even though no schema or logic would have changed. Per this sprint's own explicit instruction ("if impractical without modifying backend code, document the limitation"), this was documented rather than forced.

This does **not** leave 403 handling unverified, only *not re-demonstrated live in this session*:
- The frontend's classification is proven by an automated test using the real Contract §2.5 error envelope shape (`it('403 classifies as forbidden, not unauthorized')`, passing) — the exact same `kindForStatus`/`ApiError` code path a live 403 would go through.
- The backend's real capability to return 403 for a role- or project-mismatched request is not a claim — it is exactly what `be13_cancel_rejects_cross_project_and_insufficient_role` and `be15_findings_cross_project_isolation_and_role_check` (this engagement's own BE-13/BE-15 test suites, currently part of the 96/96-passing backend suite) already assert against the real router.
- `GET /api/v1/projects` — the endpoint FE-2's "Verify Connection" action uses (chosen because it is the one real, protected endpoint that requires no project id up front and accepts every role — see `AuthContext.tsx`'s own doc comment) — cannot itself produce 403 by design (`list_projects_handler` only filters by the caller's scope, it never denies). Demonstrating a live 403 through the UI would require wiring a role- or project-gated real action, which is FE-6-onward's job, not FE-2's.

No other limitations encountered.
