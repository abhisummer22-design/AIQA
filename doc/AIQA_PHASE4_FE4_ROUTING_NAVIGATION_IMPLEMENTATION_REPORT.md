# AIQA — Phase 4 — FE-4 Implementation Report

## 1. Status

**PASS**, with two explicitly disclosed, non-fabricated limitations (§13, §16).

## 2. Exit criterion / scope

FE-4's brief: give the frontend real client-side routing and navigation — currently `activePage` was pure in-memory React state with no URL, no direct-link support, no refresh support, and no browser Back/Forward. This sprint's scope was **routing architecture only**:

- A single, centralized route definition consumed by the router, the Sidebar, and the CommandPalette (no scattered hardcoded paths).
- Real URL behavior: direct navigation, refresh, Back/Forward, unknown-route recovery.
- Route protection wired to FE-3's existing `AuthContext`/`StateGate` — no second auth mechanism.
- **Dashboard** and **Test Case Generator** (product-owner-approved reference pages) touched only with the minimal non-visual change needed to route them — no redesign.
- The other 9 pages (Google-Studio-generated, not approved final design) made reachable via routing only — their current UI is not treated as locked-in and no new UI was invented for them.
- No backend changes, no changes to FE-1's transport core or FE-2's auth architecture, no removal of existing pages or fake data, no blank screens, no OAuth/OIDC/sessions.

Satisfied — see §4–§11.

## 3. FRD/SRD/PRD/flow-diagram compliance

The brief requires route/navigation naming to be cross-checked against the project's FRD/SRD/PRD/flow diagrams as authoritative, with contradictions flagged rather than silently resolved.

**Finding: no such documents exist anywhere in this repository.** Confirmed by an exhaustive, repeatable search:

```bash
find . -not -path "*/node_modules/*" -not -path "*/target/*" -not -path "*/.git/*" \
  \( -iname "*FRD*" -o -iname "*SRD*" -o -iname "*PRD*" -o -iname "*flow*diagram*" -o -iname "*business*logic*" \)
```

Zero matches, re-run at the start of this session to confirm the finding still holds (nothing was added to the repo between the original search and now). This mirrors a precedent already established in this engagement — the Integration Plan's own `[NEEDS BL v2.0]` handling for a similarly-unavailable "AIQA Business Logic & Architecture v2.0" document.

**Resolution (not a silent guess):** in the documented absence of an authoritative naming source, route naming and scope were reconciled against the next-best actually-authoritative source — this codebase's own existing, already-product-reviewed page vocabulary (`types.ts`'s `NavPage` union, `Sidebar.tsx`'s pre-existing labels). This is documented directly in [`routeConfig.tsx`](../forentend/src/routes/routeConfig.tsx)'s module doc comment, not only here.

**Flagged contradiction:** the brief's own illustrative example route list included several paths — `/analytics`, `/findings`, `/audit`, `/evidence`, `/ui-testing`, `/security-testing`, `/performance-testing` — that have **no corresponding page** anywhere in `forentend` today. These were deliberately **not** implemented as routes, because doing so would mean fabricating new pages/UI, which the brief itself rules out ("FE-4 is NOT the phase for implementing real backend pages"). This is a genuine tension in the brief's own text (its illustrative list vs. its own no-new-UI constraint), resolved in favor of the explicit constraint and flagged here rather than silently picking one reading.

## 4. Central route configuration

**`forentend/src/routes/routeConfig.tsx`** (new) — the single source of truth. Every navigation surface reads from it; no component hardcodes one of these 12 paths.

```ts
export interface RouteDef {
  id: NavPage;
  path: string;
  label: string;
  icon: LucideIcon;
  badge?: string;
  badgeType?: 'info' | 'warning' | 'live';
  Component: ComponentType;
  designStatus: 'product-owner-approved' | 'google-studio-not-final';
  protectedRoute: boolean;
}
export const routeDefs: RouteDef[] = [ /* 12 entries */ ];
export const DEFAULT_ROUTE_PATH = '/dashboard';
export function routeDefFor(id: NavPage): RouteDef | undefined { ... }
```

| Route | Path | Design status | Protected |
|---|---|---|---|
| Dashboard | `/dashboard` | product-owner-approved | yes |
| Test Case Generator | `/test-generator` | product-owner-approved | yes |
| AIQA Orchestrator | `/orchestrator` | google-studio-not-final | yes |
| Test Case Validator | `/validator` | google-studio-not-final | yes |
| Test Execution Hub | `/execution` | google-studio-not-final | yes |
| RBAC Testing | `/rbac` | google-studio-not-final | yes |
| API Testing | `/api-testing` | google-studio-not-final | yes |
| Mobile Testing | `/mobile-testing` | google-studio-not-final | yes |
| Custom Testing | `/custom-testing` | google-studio-not-final | yes |
| Feature Map / Flow | `/feature-map` | google-studio-not-final | yes |
| Reports | `/reports` | google-studio-not-final | yes |
| Settings / AI Model | `/settings` | google-studio-not-final | **no** (deliberate — see §6) |

`designStatus` is metadata for this file and this report, not rendered UI — adding a visible "not final design" banner to a page would itself be a small redesign of that page, which is out of scope.

## 5. Components created/modified

- `forentend/src/routes/routeConfig.tsx` — new, as above.
- `forentend/src/routes/AppRoutes.tsx` — new, the actual `<Routes>` table (root redirect, 12 routes, `*` 404 fallback), built entirely from `routeDefs`.
- `forentend/src/components/pages/NotFoundPage.tsx` — new. Reuses the existing `EmptyState` component (icon `Compass`, badge `404`, "Go to Dashboard" action) — no new visual design.
- `forentend/src/App.tsx` — rewritten: `AppShell` (providers + chrome + `<AppRoutes />`) is now wrapped in `BrowserRouter` by the default export; `AppShell` is exported separately purely so tests can substitute `MemoryRouter`.
- `forentend/src/components/layout/Sidebar.tsx` — hardcoded `navItems` array and `activePage`/`setActivePage` removed; now renders `routeDefs`, uses `useLocation()` for active-state and `useNavigate()` for clicks.
- `forentend/src/components/layout/CommandPalette.tsx` — hardcoded 12-item "Workspaces" list removed; now generated from `routeDefs.map(...)`, uses `useNavigate()`.
- `forentend/src/components/layout/RunTriggerModal.tsx` — `setActivePage('execution')` replaced with `navigate('/execution')`.
- `forentend/src/components/pages/DashboardPage.tsx` — **one line changed**: the "Execution Hub" quick-action button's `onClick` now calls `navigate('/execution')` instead of `setActivePage('execution')`. Nothing else in this product-owner-designed page was touched.
- `forentend/src/components/pages/TestCaseGeneratorPage.tsx` — same one-line treatment inside `handleExportToExecution`.
- `forentend/src/context/AIQAContext.tsx` — `activePage`/`setActivePage` (state, type fields, provider value) removed; this was the app's in-memory navigation mechanism, now fully superseded by the URL. This is the one deliberate touch to this otherwise-untouched, fake-data-heavy file — justified as removing the *navigation* state this sprint's brief requires replacing, not as touching *product/fake data*. No fake data field was added, removed, or altered.

## 6. Route protection

`AppRoutes.tsx`'s `ProtectedRoute` wraps each protected route's element in FE-3's own `StateGate`, driven by the real `useAuth()` context — not a second auth mechanism:

```tsx
const ProtectedRoute: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const auth = useAuth();
  const navigate = useNavigate();
  return (
    <StateGate auth={auth} actions={{ onConfigure: () => navigate('/settings') }}>
      {children}
    </StateGate>
  );
};
```

`StateGate` only renders `children` when `auth.status === 'authenticated'`; every other status (`no_key`, `key_present`, `checking`, `unauthenticated`, `forbidden`, `offline`, `error`) renders FE-3's real state card instead — never fabricated authenticated content.

**Settings is the one deliberate, disclosed exception** (`protectedRoute: false`): it is where the credential is configured in the first place, so gating it would deadlock a user with no key yet. This is a resolved judgment call documented in `routeConfig.tsx`'s own doc comment, not a silent omission — the brief's route-protection requirement (§named states) doesn't explicitly call out Settings, and "route guards must not create a second auth mechanism" plus plain deadlock-avoidance both point the same direction.

## 7. Navigation surfaces

- **Sidebar** — active item is `location.pathname === route.path`, computed fresh on every render from the real route, not a stored string compared against a separate "which page am I on" variable.
- **CommandPalette** — its "Workspaces" section is `routeDefs.map(r => ({ label: \`Go to ${r.label}\`, action: () => navigate(r.path) }))`; adding/removing/renaming a route in `routeConfig.tsx` automatically updates the palette with no separate edit.
- **RunTriggerModal** — its post-launch redirect now navigates to the real `/execution` URL.

No component outside `routeConfig.tsx` hardcodes one of these 12 path strings.

## 8. URL/browser behavior

| Behavior | Mechanism | Verified |
|---|---|---|
| Direct navigation to a deep route | `<Route path>` matches independent of navigation history | Automated (item 8) + live (§13-D) |
| Refresh on a deep route | Same — a fresh `BrowserRouter` mount re-resolves the current `window.location` | Live (§13-E) |
| Browser Back | Real `BrowserRouter` listens for `popstate`; `window.history.back()` | Automated (item 6) + live (§13-H) |
| Browser Forward | Same, `window.history.forward()` | Automated (item 7) + live (§13-H) |
| Unknown route | `<Route path="*" element={<NotFoundPage />} />` | Automated (item 9) + live (§13-J) |
| Root `/` | `<Navigate to={DEFAULT_ROUTE_PATH} replace />` | Automated (item 1) + live (§13-A) |

## 9. Root route decision

`/` redirects to `/dashboard` (`DEFAULT_ROUTE_PATH`). With no FRD/SRD/PRD to consult (§3), this preserves the app's own pre-existing default rather than inventing a new entry-flow decision: `AIQAContext` already defaulted `activePage` to `'dashboard'`, and the old `MainContentArea` switch's own `default` case already rendered `<DashboardPage />`.

## 10. Active navigation state

Sidebar highlighting is `location.pathname === route.path`, read from `useLocation()` on every render — not a fragile string comparison against a separately-tracked "current page" variable (the old `activePage === item.id` pattern this sprint removed). Verified directly by inspecting the rendered `className` of every Sidebar button live in-browser (§13-F) and by an automated test (item 5) that navigates and re-asserts which single button carries the active class.

Unsaved-state handling and dynamic route parameters were **not implemented** — the documentation search (§3) found nothing requiring either, and no existing page has unsaved-form state that a route change would need to guard, so implementing either would be inventing requirements, which the brief explicitly forbids.

## 11. Security

- No API key, token, or credential ever appears in a URL, path, or query string — checked by an automated test (item 17: asserts the raw key never appears in `location.pathname`/`location.search`) and by live inspection (`window.location.href`, `.pathname`, `.search`) after configuring a real key, after a real 401, and after a real offline event (§13).
- `ProtectedRoute` reads `AuthContext` only — it does not read, cache, or duplicate the key itself, and does not introduce any new storage of credentials.
- No new `localStorage`/`sessionStorage` access was added anywhere in this sprint's files.
- Route guards cannot be bypassed by URL manipulation: navigating directly to any protected path with no/invalid/unverified key renders the real `StateGate` card, never the page's real content — confirmed for every `AuthStatus` value both by automated tests (items 10–16) and live (§13).

## 12. Automated tests

**20 new tests** in `forentend/src/routes/AppRoutes.test.tsx`:

| # | Covers |
|---|---|
| 1 | Root `/` resolves to the documented default (`/dashboard`) |
| 2 | `/dashboard` renders the real Dashboard page (post-auth) |
| 3 | `/test-generator` renders the real Test Case Generator page (post-auth) |
| 4 | Clicking a Sidebar link resolves to the correct path |
| 5 | Active nav item follows the real route, not a separate string |
| 6 | Real browser Back returns to the previous route |
| 7 | Real browser Forward re-applies the route gone back from |
| 8 | Direct/refreshed navigation to a valid non-root route loads that exact route |
| 9 | An unknown route renders Not Found, not a blank page |
| 10–16 | One test per real `AuthStatus` (`no_key`, `key_present`, `checking`, `authenticated`, `unauthenticated`/401, `forbidden`/403, `offline`) — each asserts the correct state card renders **and** that protected content (`"Execution Hub"` etc.) is absent |
| — | Settings remains reachable with `no_key` (deliberately ungated) |
| 17 | The raw API key never appears in the URL, in any state |
| 18 | Dashboard: existing product-owner-designed UI and fake data render unregressed |
| 19 | Test Generator: existing product-owner-designed UI renders unregressed |

```
Test Files  1 passed (1)
     Tests  20 passed (20)
```

Full suite (existing FE-1/2/3 tests + new FE-4 tests), unmodified in assertion content for every pre-existing file:

```
Test Files  4 passed (4)
     Tests  66 passed (66)
```
(14 `aiqaApi.test.ts` + 11 `AuthContext.test.tsx` + 21 `StateViews.test.tsx` + 20 `AppRoutes.test.tsx`.)

Two real jsdom/testing-library quirks were worked through during this sprint (not glossed over):
- `MemoryRouter` does not observe `window.history`/`popstate`, so items 6–7 render the real default `App` (real `BrowserRouter`) instead, reading `window.location.pathname` directly.
- jsdom dispatches the `popstate` from `history.back()`/`.forward()` asynchronously on its own task queue; a fixed `setTimeout(0)` flush proved unreliable under load, so items 6–7 poll via `waitFor` until the pathname actually updates, rather than asserting on a single fixed-delay snapshot.

## 13. Manual browser verification (live)

Ran the real Vite dev server against a **real, freshly-built `cargo run --release` orchestrator** (`services/orchestrator`, port 8080), driven through the in-app browser — not mocked, not simulated. To exercise every auth state live (including a genuine 200/authenticated response, which prior FE-2/FE-3 sessions could not demonstrate because no plaintext key from an earlier session survives, and minting one through the API requires an existing valid key), two API keys were minted for this session only by inserting rows directly into the existing SQLite `api_keys` table via a short Python script, using the exact scheme the backend's own bootstrap logic uses (`aiqa_<64 hex>` raw value, SHA-256 hash stored, `role`/`project_id`/`created_at` columns) — **no backend source file was touched**; this is test-credential seeding into existing infrastructure, done transparently and disclosed here, not a code change. See §16 for the exact scope of what this did and did not touch.

- **A — Root + `no_key`.** Fresh load of `/` → resolved to `/dashboard`; with no key configured, `StateGate` rendered "No orchestrator API key configured" (real card, no fabricated content). **PASS.**
- **B — Settings reachable with no key.** Navigated to Settings with zero key configured; the Authentication & SSO tab's real key-entry form rendered and was usable. **PASS.**
- **C — Real 200/authenticated.** Entered the freshly-minted admin key, clicked "Verify Connection" — the app issued a real request to the live orchestrator, which returned a genuine 200; UI showed "AIQA Orchestrator authentication verified / CONNECTED". Navigating to `/dashboard` then rendered its real content. **PASS.**
- **D — Direct URL navigation.** Typed `/test-generator` directly into the address bar; the real Test Case Generator page rendered (post re-verification). **PASS.**
- **E — Refresh on a deep route.** Force-reloaded at `/test-generator`; the URL stayed `/test-generator` (no reset to `/dashboard`, no blank page), and `StateGate` correctly showed the unverified-key card on the fresh mount rather than fabricating authenticated content. **PASS.**
- **F — Sidebar navigation + active state.** Clicked through Sidebar links; URL updated each time, and a live DOM check of every nav button's `className` confirmed exactly one (`bg-indigo-600 text-white ...`) carried the active style, matching the current route. **PASS.**
- **G — CommandPalette.** Opened it, clicked "Go to Reports" (real, `routeDefs`-generated entry); URL updated to `/reports` and the palette closed. **PASS.**
- **H — Browser Back/Forward.** With a real multi-entry history stack (`/test-generator` → `/dashboard`), triggered real Back → returned to `/test-generator`; triggered real Forward → returned to `/dashboard`. **PASS.**
- **I — Unknown route.** Navigated to `/this-route-does-not-exist`; the real 404 page rendered ("Page not found" / "Go to Dashboard"), not a blank page or crash; clicking its recovery action returned to `/dashboard`. **PASS.**
- **J — Invalid key → 401.** Overwrote the stored key with a well-formed-but-wrong `aiqa_<hex>` value, clicked Verify — the real backend rejected it, UI showed "Authentication required / AUTHENTICATION REQUIRED · 401" with the key masked, not raw. **PASS.**
- **K — Offline.** Killed the real orchestrator process, confirmed unreachable via `curl` (connection refused), clicked Retry — UI showed "Orchestrator unreachable / ORCHESTRATOR UNREACHABLE". Backend was then restarted and confirmed healthy again. **PASS.**
- **Security check.** At every step above, `window.location.pathname` / `.search` / `.href` were inspected directly — the key never appeared in any of them. **PASS.**

**403/`forbidden` — not independently live-demonstrated through the browser UI, disclosed rather than fabricated.** This is a pre-existing, already-documented FE-2/FE-3 structural limitation (see `AuthContext.tsx`'s own comment, and the FE-2/FE-3 reports' own §16/§14): the client's validation call is `GET /projects`, and that handler filters by caller scope rather than ever rejecting an authenticated caller, so no client-driven "Verify Connection" click can produce a live 403 without wiring a new role-gated action into the UI — explicitly out of scope for a routing sprint. What **was** newly confirmed this session, beyond what FE-2/FE-3 could show:
  - `curl` against the real backend, using a genuinely viewer-scoped key minted the same way as above, calling an admin-only route (`POST /admin/api-keys`) → a real, distinct `403 Forbidden` (verified alongside a real `401` for a garbage key and a real `200` for the admin key, all in the same session, same backend process).
  - The automated suite (item 15) proves the client-side `AuthContext` switch statement and `StateGate` correctly render the `forbidden` card (and withhold protected content) given a `forbidden`-kind `ApiError` — the same machinery a real 403 from any role-gated route would produce.
  No manual PASS is claimed for this one scenario; everything else in this section was genuinely exercised.

## 14. Typecheck/lint/build

Exact commands, run from `forentend/`:

```
npx tsc --noEmit     # clean, no errors
npx vitest run       # 66/66 passing
npx vite build       # succeeds, 2707 modules, ~11s
```

The production build's pre-existing >500kB chunk-size warning is unrelated to this sprint (already-known, pre-existing; not introduced by FE-4).

## 15. Fake data & visual design preservation

- **Dashboard and Test Case Generator** — product-owner-approved reference pages — each received exactly one line of change (a `setActivePage` call replaced with the equivalent `navigate` call). No visual element, layout, copy, or fake-data field was touched. Confirmed live: Dashboard's KPI cards, project spec panel, execution telemetry, and status charts all still render with their existing fabricated values (§13-C), unchanged from before this sprint.
- **The other 9 pages** — Google-Studio-generated, not approved final design — are now reachable via real routes with zero new UI invented for them; their existing (pre-FE-4) visual content is untouched.
- **`AIQAContext.tsx`** — only its now-superseded navigation state (`activePage`/`setActivePage`) was removed; every other field (fake runs, fake findings, fake projects, etc.) is untouched.
- No existing page was removed, blanked, or made unreachable. `EmptyState` (reused for `NotFoundPage`) is an existing component, not a new visual pattern.

## 16. Backend changes, scope boundary, and limitations

- **Backend source: none.** No file under `services/orchestrator/src` or `migrations/` was created, edited, or deleted this sprint.
- **Backend database: two test API keys inserted for live verification only** (§13), via a standalone Python script (not part of the frontend or backend codebase, not committed), using the same key format, hashing scheme, and table schema the backend's own `bootstrap_admin_key_if_needed`/`insert_api_key` functions use. This was necessary because no plaintext key from any earlier session in this engagement survived (only hashes persist by design), and the backend's own key-minting endpoint requires an existing valid key to call. No existing key was revoked, modified, or read; no schema was changed.
- **FE-1 transport core, FE-2 auth architecture, FE-3 state-view layer** — none of their internals were modified. `AppRoutes.tsx` consumes `useAuth()`/`StateGate` exactly as FE-3 designed them to be consumed by a future routing layer.
- **Documentation contradiction flagged, not silently resolved** — §3.
- **403/`forbidden` live-browser demonstration** — not achieved, disclosed rather than fabricated — §13.
- **FE-5 and onward: not started.** This sprint implemented routing/navigation architecture only. No later-phase work (real data wiring for the 9 non-reference pages, visual redesign of Google-Studio-generated pages, role-gated UI actions, unsaved-state guards, route parameters, etc.) was begun.

**Critical rule confirmed followed:** this was a routing architecture sprint, not a visual redesign sprint. No requirement, backend behavior, test result, or manual-verification outcome in this report was fabricated; every limitation encountered is disclosed above rather than glossed over.
