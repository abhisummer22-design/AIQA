# AIQA — Phase 4 — FE-3 Implementation Report

## 1. Status

**PASS**

## 2. Exit criterion

FE-3's exit criterion (Integration Plan §16): *"Loading/Empty/Error/Unauthorized/Offline/NeedsReview in `forentend`'s design system."* This sprint's brief scoped FE-3 specifically to the **authentication** state set FE-2 introduced (`no_key`/`key_present`/`checking`/`authenticated`/`unauthenticated`/`forbidden`/`offline`/`error`), not the full generic resource-loading six-state model (Loading/Empty/Error/Unauthorized/Offline/NotImplemented) that a later unit will need for real data resources (runs, findings, evidence, etc.). All eight authentication states now render through a single, reusable, real-state-driven component layer, integrated into the existing Settings UI, with a `StateGate` pattern ready for future FE units to reuse for the generic case. Satisfied — see §3–§6.

## 3. State architecture

- **`forentend/src/components/state/StateViews.tsx`** (new) — the reusable state-view layer.
- **`renderAuthState(auth: AuthContextType, actions?): ReactNode`** — the single mapping function (one `switch (status)`, nowhere else in the codebase branches on `AuthStatus` to pick a component). A `never`-typed exhaustiveness guard in its `default` case means adding a ninth `AuthStatus` value without updating this function fails the build, not silently renders nothing.
- **`<StateGate auth={...}>{children}</StateGate>`** — renders `children` only when `auth.status === 'authenticated'`; every other state renders `renderAuthState`'s output instead. Not wired into any real page this sprint (brief §5) — it exists so the next unit that wires a real page doesn't reinvent this or duplicate the state mapping.
- Every action callback (`onVerify`, `onSignOut`) defaults to the real `AuthContext` action (`auth.validate`, `auth.signOut`) when the caller doesn't override it — `renderAuthState(auth)` alone, with no second argument, is already a fully real, non-mock render.

## 4. Components created/modified

- `forentend/src/components/state/StateViews.tsx` — new. `AuthStateCard` (shared presentational primitive, styled to match `common/EmptyState.tsx`'s existing card language), eight per-state components, `Cta` button primitive, `renderAuthState`, `StateGate`.
- `forentend/src/components/state/StateViews.test.tsx` — new, 21 tests.
- `forentend/src/context/AuthContext.tsx` — one-line change: `interface AuthContextType` → `export interface AuthContextType` (required for `StateViews.tsx` to type its props against the real context shape; no behavioral change, no new export of anything else).
- `forentend/src/components/pages/SettingsPage.tsx` — `OrchestratorAuthPanel` refactored: the ad-hoc badge/banner switch statement (duplicated state-to-UI logic) is removed and replaced with a single `{renderAuthState(auth, { onConfigure: () => setShowInput(true) })}` call. The component now owns only what is genuinely Settings-specific: the key-entry input and its Save/Replace button. Save/Replace/masked-key/existing-SSO-content behavior is unchanged.

## 5. Authentication integration

`OrchestratorAuthPanel` calls `useAuth()` once and passes the whole context value into `renderAuthState`, with one override: `onConfigure`, which reveals the panel's own local key-entry input (`setShowInput(true)`) — this is the one piece of UI that is genuinely page-specific (only a page with an input field can offer "configure/replace" inline; a future read-only page might instead navigate to Settings, which is exactly why `onConfigure` is an optional override rather than baked into the shared layer). `onVerify`/`onSignOut` are left at their defaults (`auth.validate`/`auth.signOut`) everywhere.

## 6. UI states

All eight, each a real card (icon, uppercase status badge, title, description, masked key where applicable, action buttons) matching the existing dark/`#0B1120`/`slate-800` visual language:

| Status | Badge | Title | Actions |
|---|---|---|---|
| `no_key` | NO KEY CONFIGURED | No orchestrator API key configured | Configure API Key (if `onConfigure` given) |
| `key_present` | KEY SAVED · UNVERIFIED | Key saved, not yet verified | Verify Connection, Sign Out |
| `checking` | VERIFYING… | Verifying connection | *(none — see below)* |
| `authenticated` | CONNECTED | AIQA Orchestrator authentication verified | Verify Connection, Sign Out |
| `unauthenticated` | AUTHENTICATION REQUIRED · 401 | Authentication required | Replace Key, Retry, Sign Out |
| `forbidden` | ACCESS FORBIDDEN · 403 | Access forbidden | Retry, Sign Out *(no Replace Key)* |
| `offline` | ORCHESTRATOR UNREACHABLE | Orchestrator unreachable | Retry, Sign Out |
| `error` | ERROR | Something went wrong | Retry, Sign Out (+ `request_id` if present) |

Notable implementation choices:
- **`checking` renders zero buttons, not a disabled one.** This is what actually prevents a duplicate validation request while one is in flight — there is no button to double-click, eliminating the disabled-prop race rather than relying on timing.
- **`authenticated` shows no role/project data.** `AuthContext`'s validation call (`GET /api/v1/projects`) never captures or stores a role/project claim, so there is nothing real to show — inventing one would violate the brief's own "do not invent role/project information" rule.
- **`forbidden` deliberately omits "Replace Key".** Per the brief: a 403 is not "authentication missing," so this state must not nudge the user toward re-entering a credential that is already valid.
- **401 vs 403 are distinguished by more than color**: different icons (`XCircle` vs `ShieldAlert`), different badge text (`· 401` vs `· 403`), different titles, and materially different descriptions — verified by an automated test that asserts the two cards' text content is never equal.

## 7. Security

Checked across every file this sprint touched (`StateViews.tsx`, `StateViews.test.tsx`, `AuthContext.tsx`, `SettingsPage.tsx`):
- No `console.log`/`debug`/`info` of any kind.
- No hardcoded `aiqa_<hex>`-shaped literal.
- No direct `localStorage.*` call outside `services/aiqaApi.ts` (the two `localStorage` hits found by search are both inside doc comments, not code).
- No manually-constructed `Authorization` header anywhere in a UI component — every request still goes through the existing FE-1 transport core.
- No raw key ever rendered: `AuthStateCard` only ever receives `maskedKey` (a `string | null` already masked before it reaches this layer); verified by an automated test that renders every one of the eight states and asserts the raw key string, the literal word "authorization", and the literal "bearer " never appear in the rendered HTML.
- No telemetry/analytics of any kind was added.
- Live in-browser check (§9): after saving a real (fake-value) key and later signing out, `document.body.innerHTML` was searched for the raw value — not found — and `localStorage.getItem('aiqa.orchestrator.apiKey')` was confirmed `null` post-sign-out.

## 8. Tests

**21 new tests** in `src/components/state/StateViews.test.tsx`, all passing:
- Items 1–8 (one card per real `AuthStatus`) — 8 tests.
- Items 9–13 (401/403 distinctness; raw key never appears across all 8 states; masked key persists through 401/403/offline) — 5 tests.
- Items 14–15 (rendering triggers zero automatic `validate`/`saveKey` calls; `checking` renders no actionable control at all; `Verify Connection` is present and clickable in `key_present`) — 3 tests.
- `StateGate` gate-only-when-authenticated — 1 test.
- "Driven by the real state machine" (critical rule) — 3 tests: a full live flow through the real `AuthProvider` (no_key → save → checking → authenticated → sign out → no_key); a real 401-then-403 sequence proving the *view* tracks real, distinct backend responses; a real network-failure-classifies-as-offline flow.
- Bonus regression guard (found during manual verification, see §9) — 1 test: Sign Out is reachable and functional from every state where a key exists.

```
 Test Files  3 passed (3)
      Tests  46 passed (46)
```
(25 pre-existing FE-1/FE-2 tests + 21 new FE-3 tests.)

## 9. Manual verification

Ran the real dev server against a **real, live `cargo run --release` orchestrator** (not mocked), driven via the in-app browser, for every scenario the brief lists.

- **Scenario A (`no_key`)** — Exercised live. PASS.
- **Scenario B (`key_present`)** — Exercised live (saving a key immediately showed the masked value and the unverified badge). PASS.
- **Scenario C (`checking`)** — **Attempted live, not reliably captured.** Two attempts to read the DOM immediately after clicking Save/Retry both landed after the transition had already resolved — the real localhost round-trip (well under the tool's own read latency) is too fast to catch mid-flight through this browser-automation path. The state itself is not unverified: it is deterministically proven by an automated test that holds an unresolved `fetch` promise open and asserts the `VERIFYING…` card renders before resolving it (§8).
- **Scenario D (`authenticated`)** — **Not exercised live against a literal 200 response.** Same root cause as FE-2's undemonstrated 403: the persistent orchestrator database already has API keys from this engagement's prior sessions, no plaintext key from an earlier session was recoverable, and minting a fresh one would require either wiping existing keys (destructive) or writing to the database outside the backend's own API (see the FE-2 report §16 for the full reasoning, unchanged here). The real-context-driven test suite (§8) proves the `authenticated`/`CONNECTED` card renders correctly given a real 200 response through the real `AuthProvider` state machine.
- **Scenario E (`unauthenticated`/401)** — Exercised live against the real backend: saved an invalid key, the app issued a real `GET /api/v1/projects` through the dev proxy, the live orchestrator returned a genuine `401 Unauthorized` (confirmed via the browser's own network log), and the UI showed "AUTHENTICATION REQUIRED · 401" with the key still configured. PASS.
- **Scenario F (403)** — **Not exercised live.** Same limitation as D and as FE-2's own Scenario F — see the FE-2 report §16. Backed instead by: an automated test that mocks a real Contract §2.5 403 envelope and asserts the distinct `ACCESS FORBIDDEN · 403` card; and this engagement's own already-passing backend tests (`be13_cancel_rejects_cross_project_and_insufficient_role`, `be15_findings_cross_project_isolation_and_role_check`) proving the backend really does produce 403 for role/project mismatches. **403 live verification not exercised — documented, not fabricated.**
- **Scenario G (`offline`)** — Exercised live: the orchestrator process was actually killed (`Stop-Process`), confirmed unreachable via `curl` (exit code 7), then "Retry" was clicked in the browser. The dev proxy's own 502-on-upstream-down handler fired for real (confirmed in the network log: `GET /api/v1/projects → 502 Bad Gateway`), and the UI showed "ORCHESTRATOR UNREACHABLE" with the key still configured. PASS.
- **Scenario H (sign out)** — Exercised live: clicking "Sign Out" returned the UI to the exact `NO KEY CONFIGURED` state, and `localStorage` was confirmed empty afterward. PASS.
- **Raw key never in the DOM** — Checked live via `document.body.innerHTML` search after save/verify/sign-out cycles: not found, in addition to the automated coverage in §8.

**Bug found and fixed during this verification**: the first draft of `StateViews.tsx` only rendered a "Sign Out" action inside the `authenticated` card, silently regressing FE-2's behavior (where Sign Out was always available whenever a key existed) — a user stuck in `key_present`/401/403/offline/error had no way to clear their credential from the state card. Found live while testing Scenario H from the `offline` state. Fixed by adding Sign Out to every state where `hasKey` is true, and added a dedicated regression test (§8) so this class of bug fails the suite if it recurs.

## 10. Typecheck/lint/build

Exact commands, run from `forentend/`:

```
npx tsc --noEmit     # clean, no errors
npx vitest run       # 46/46 passing
npx vite build       # succeeds, 2693 modules, ~6-7s
```

The production build's pre-existing >500kB chunk-size warning is unrelated to this sprint (already-known Integration Plan risk R10).

## 11. Regression

All 25 pre-existing FE-1/FE-2 tests (14 in `aiqaApi.test.ts`, 11 in `AuthContext.test.tsx`) still pass, unmodified in assertion content — the only FE-2 file touched (`AuthContext.tsx`) received a single additive export keyword, not a logic change. No FE-1/FE-2 test was weakened, skipped, or removed.

## 12. Fake data preservation

Confirmed preserved, not touched:
- `AIQAContext.tsx` — not modified at all (not even opened for editing this sprint).
- `SettingsPage.tsx`'s pre-existing fake SSO toggle, IdP dropdown, and "Control Plane Daemon API Token / Status: ACTIVE • Expires in 180 days / Rotate Token" card — still rendered exactly as before, confirmed live in-browser in every scenario in §9 (visible below the real auth panel in every screenshot-equivalent `get_page_text` capture).
- No existing page was removed or made blank.

## 13. Backend changes

**None.** `services/orchestrator` was not modified. The orchestrator used for live verification (§9) was the existing `cargo run --release` binary from FE-2's own verification, started and stopped for this session's checks only. Its persistent database was read from (via real HTTP requests it served) but never written to by this sprint's own code, and no key was added, removed, or revoked.

## 14. Limitations

- **Scenario C (`checking`)** was attempted live but not reliably captured due to localhost round-trip speed — deterministically proven instead by an automated test with controlled timing (§8/§9).
- **Scenario D (`authenticated`) and Scenario F (`forbidden`/403)** were not exercised live against a real backend response in this session, for the same reason documented in the FE-2 report §16 (no recoverable valid API key against the shared persistent database, and minting a new one without violating "do not modify backend/database" was not practical). Both are backed by real-context-driven automated tests and, for 403 specifically, by this engagement's own already-passing backend test suite.

No other limitations encountered. No backend code or persistent database state was modified to manufacture any test result.

## 15. Out-of-scope

Not started: FE-4 (routing), FE-5 (global project scope), FE-6 (audit page), FE-7 (evidence viewer), FE-8 (execution hub real), FE-9 (realtime), FE-10 (dashboard real), FE-11 (fabrication removal), FE-12 (status vocabulary). No RBAC user-management UI, OAuth/OIDC, sessions/refresh tokens, or WebSocket work was introduced. `StateGate` was built for future units to reuse but was not wired into Dashboard, Projects, Audit, Evidence, Execution Hub, Findings, or Reports in this sprint.
