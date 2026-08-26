# AIQA — Phase 4 — FE-5 Implementation Report

## 1. Status

**PASS**

## 2. Exit criterion

FE-5's exit criterion (this sprint's brief): *"a real, centralized Project Scope architecture... once authenticated, the currently selected AIQA project is available consistently across the application."* Satisfied — a single `ProjectContext`/`ProjectProvider` (§3) sits in the real provider tree, obeys FE-2/FE-3's real `AuthStatus` state machine exactly (§8), fetches only the real, backend-returned, caller-accessible project list through FE-1's existing `aiqaApi.projects.list()` (§4), and exposes `selectedProject`/`selectedProjectId` globally via `useProject()` to every component in the tree, including both product-owner-approved pages (§9, §10) without redesigning either.

## 3. Project-scope architecture

**`forentend/src/context/ProjectContext.tsx`** (new) — `ProjectProvider` + `useProject()`, matching the brief's own suggested interface almost verbatim:

```ts
export interface ProjectContextValue {
  projects: Project[];               // real backend Project[] (types/contract.ts)
  selectedProjectId: string | null;
  selectedProject: Project | null;   // derived, never invented
  loading: boolean;
  error: ApiError | null;            // FE-1's real ApiError, not a new error type
  selectProject: (projectId: string) => void;
  refreshProjects: () => Promise<void>;
}
```

No field was invented beyond what the brief itself specified; `selectedProjectId`/`selectedProject` can be `null` (brief: "Do not invent fields that are not present in the real backend response" — `null` is the honest state when nothing is confirmed-selected, not an invented placeholder project).

## 4. Backend/API integration

Exactly one real endpoint is consumed: **`GET /api/v1/projects`** via the existing FE-1 abstraction, `aiqaApi.projects.list(signal)` — no second fetch/request mechanism, no direct `fetch()` in any page or component. `list_projects_handler` (`services/orchestrator/src/api.rs:235`) already filters by `ctx.project_id` server-side (`state.store.list_projects(ctx.project_id.as_deref())`), so a project-scoped key correctly returns only its one accessible project and a global key returns everything real — confirmed live (§15, Scenario C) with a genuinely project-scoped key minted via the existing `POST /admin/api-keys` endpoint, not fabricated client-side.

## 5. Project selection

- Zero projects (Case A): `projects: []`, `selectedProject: null`, no error — a real, distinct empty state, not a fabricated project (§17).
- Exactly one project (Case B): **not auto-selected.** The brief explicitly warns "do not assume [auto-selection] unless supported by the product requirements or existing architecture" — no FRD/SRD/PRD exists (§18) and no existing precedent for auto-select was found in the codebase (`AIQAContext`'s own fake `selectedProjectId` default is a hardcoded literal, not a "select when N=1" rule). Resolved conservatively: the user always makes an explicit selection, confirmed live against a real project-scoped key returning exactly one project (§15, Scenario C).
- Multiple projects (Case C): the real `ProjectSelector` dropdown lists every real, backend-returned project; clicking one calls `selectProject(id)`.
- Stale/inaccessible selection (Cases D/E): `selectProject` only ever accepts an id present in the current real `projects` list — an id that was persisted, guessed, or otherwise doesn't resolve is silently never adopted (§7, §18: "project ID/name cannot be trusted as proof of permission"). Every fresh fetch (`applyList`) re-validates the current selection against the real list and drops it if absent.

## 6. Project persistence

No FRD/SRD/PRD exists to specify persistence rules (§25), so the smallest safe behavior was chosen and is documented here and in `ProjectContext.tsx` itself: **only the opaque project id** (never the project object, never anything credential-shaped) is persisted, in its own `localStorage` key (`aiqa.selectedProjectId`) — deliberately separate from `aiqaApi.ts`'s own `aiqa.orchestrator.apiKey`, so this is not a second credential-storage architecture, just the same mechanism used for a different, non-sensitive value.

Survival rules (all centralized inside `ProjectContext.tsx`, no `localStorage` call anywhere else):

| Event | Persisted id |
|---|---|
| Page refresh (same key, same project) | **Survives** — re-adopted once re-authentication succeeds and the id is confirmed present in the real list again (never blindly, per §16's own phrase — see `applyList`). Verified live (§15, Scenario E). |
| Sign out | **Wiped.** Verified live (§15, Scenario F) and by automated test (item 16). |
| A proven-bad credential (real 401) | **Wiped** — "project scope does not pretend to remain authenticated" (§16). Verified live (§15, Scenario G) and by automated test (item 10/12). |
| `key_present` (new/unverified credential) or `forbidden` (403) | **Left in storage**, not adopted into memory until re-validated — the credential hasn't been proven bad, only not-yet-proven-good, so a future successful authentication may still re-adopt it, subject to the same real-list membership check. Automated test items 3, 13. |
| `offline` / `error` | **Preserved as-is, in memory too** — brief §5: "offline: preserve safe local scope state... do not fabricate a fresh project list." The identity/authorization context hasn't changed, only reachability has. Verified live (§15, incidental real backend outage) and by automated test item 11. |

## 7. Project switching

`selectProject` updates `selectedProjectId` synchronously; `selectedProject` is derived (`useMemo`) so it is never stale relative to `selectedProjectId`/`projects`. Both product-owner-approved pages (Dashboard, Test Generator) key their root element on the real `selectedProjectId` (`realProjectId ?? 'no-real-project-selected'`), so any local component state on those pages resets on a real project switch rather than silently continuing to reflect a project no longer in scope (§14: "do not accidentally retain... state from the previous project"). Switching does not touch `AuthContext` (no unrelated authentication change) and does not reload the application (`react-router`'s existing tree stays mounted; only the two keyed subtrees remount).

## 8. Authentication integration

`ProjectProvider` reads the real `useAuth()` (FE-2) and reacts only to `auth.status` — it introduces no second authentication state machine (critical rule §8):

| `AuthStatus` | Behavior |
|---|---|
| `no_key` | No fetch. In-memory scope cleared; persisted selection wiped. |
| `key_present` | No fetch (§5: "do not treat the user as authenticated merely because a key exists"). In-memory scope cleared; persisted selection preserved. |
| `checking` | No-op — no fetch (avoids a duplicate in-flight request), no clear (avoids UI flicker on a same-key re-verify). |
| `authenticated` | Real fetch via `aiqaApi.projects.list()`. |
| `unauthenticated` (401) | No fetch. Cleared, persisted selection wiped. |
| `forbidden` (403) | No fetch. Cleared, persisted selection preserved (key itself is still valid — FE-3: "the key stays configured"). |
| `offline` | No fetch. Nothing cleared — last-known-good scope preserved. |
| `error` | No fetch. Nothing cleared — last-known-good scope preserved; `ProjectContext`'s own `error` (from a prior fetch, if any) is exposed separately from `AuthContext.lastError`. |

`refreshProjects()` no-ops outside `authenticated` — recovering from an auth-level failure (offline/401/...) is `AuthContext.validate()`'s job (the same real "Verify Connection"/"Retry" action FE-2/FE-3 already provide), not a second recovery path invented here.

## 9. Dashboard integration

Dashboard's own pre-existing UI, KPI cards, charts, and internal project selector are **byte-for-byte unchanged in markup/styling** from FE-4. They remain entirely driven by `AIQAContext`'s pre-existing fabricated `AIQAProject[]` data (`.metrics`, `.activeRun`, `.environment`, `.slug`, `.timelineData` — none of which exist on the real backend `Project` type `{id, name, description, created_at, updated_at}`). Wiring the real selector into that UI would mean either inventing those fields (forbidden, §19) or prematurely building real per-project KPI API wiring (explicitly out of scope, §11/§12: "expose the selected project through context but do not prematurely implement"). The one real, non-visual integration: the page's root element is keyed on the real `selectedProjectId` (§7). Confirmed live and by automated test (item 19) that Dashboard's design is pixel-identical to before FE-5.

## 10. Test Generator integration

Same treatment and same reasoning (§9). This page has **zero real `aiqaApi` calls anywhere** — its entire generation pipeline is a local `setTimeout` simulation — so brief §12's "project-dependent actions must use the selected project when the real backend contract requires it" currently has no real action to wire; `realProjectId` is exposed via `useProject()` for that future wiring and used today only as the same remount key. Confirmed live and by automated test (item 20) that its design is unregressed.

## 11. Other pages

The nine Google-Studio-generated pages (Orchestrator, Validator, Execution Hub, RBAC, API Testing, Mobile Testing, Custom Testing, Feature Map, Reports) were **not modified** this sprint and are **still not treated as approved final designs** — unchanged from FE-4's own classification (`routeConfig.tsx`'s `designStatus` field). `useProject()` is available to any of them via the same global `ProjectProvider`, but none was wired to it — doing so would mean building real per-project functionality for pages whose current UI itself isn't approved, which is out of FE-5's scope.

## 12. Fake data separation

- `types/contract.ts`'s `Project` (real: `{id, name, description, created_at, updated_at}`) vs. `types.ts`'s `AIQAProject` (fake: adds `metrics`, `activeRun`, `environment`, `slug`, `timelineData`, `status`, ...) — two distinct, pre-existing types; `ProjectContext.tsx` imports only the former.
- `AIQAContext.tsx` was **not modified** this sprint (not even opened for editing) — its fabricated `projects`/`selectedProjectId`/`selectedProject` (seeded with `prj-aiqa`/"AIQA Platform" and four other fake entries) remain exactly as they were, still driving Dashboard's own internal selector and KPI cards, entirely separate from `ProjectContext`.
- Verified by automated test (item 21): the real `ProjectContext.projects` list never contains `prj-aiqa` / "AIQA Platform" after a real fetch, and by construction (only ever populated from `aiqaApi.projects.list()`'s real JSON response) never could.

## 13. Security

- No raw API key ever passed to, stored in, or rendered by `ProjectContext` or `ProjectSelector` — `ProjectContext` never touches `aiqaApi.ts`'s key storage at all (it only reads `useAuth().status`); `ProjectSelector` reads `useAuth()` only for its `status` value. Verified live (masked-key check via DOM search) and by automated test (item 17: raw key searched for in the full rendered DOM after a real fetch — not found).
- No project id, name, or any other value was ever placed in `window.location` — selection is context-only (§15: no FRD/SRD/PRD documents a URL/route requirement, so scope stays centralized in context rather than inventing route semantics). Verified live (`window.location.href`/`.search`/`.pathname` inspected after a real selection) and by automated test (item 18).
- No project data is ever fetched before `auth.status === 'authenticated'` (§8) — enforced structurally by the single `useEffect` gate, not by a UI-level hide.
- The project list is never client-filtered or client-augmented — it is rendered exactly as the backend returned it; `selectProject` refuses any id not present in that real list, so the UI can never assert access to a project the backend didn't actually return (§18: "project ID/name cannot be trusted as proof of permission" — the backend remains the sole authorization authority, this context only mirrors its answer).
- `aiqa.selectedProjectId`'s `localStorage` key is disjoint from `aiqa.orchestrator.apiKey` — no credential-storage architecture was duplicated or touched.

## 14. Tests

**20 new tests**, all real (no snapshot/mock-only assertions of internal implementation):

- `forentend/src/context/ProjectContext.test.tsx` — **14 tests** covering brief §20 items 1-16 and 21 (grouped: items 1/2, 3, 4/5, 6/7/8, 9, 10/12, 13, 11, 14, 15, 16, 21, plus two additional tests for `refreshProjects`'s no-op-outside-authenticated guarantee and full refresh-persistence round-trip), exercised through the real `AuthProvider` + `ProjectProvider` + `useAuth`/`useProject`, `fetch` mocked only at the transport boundary.
- `forentend/src/components/project/ProjectSelector.test.tsx` — **6 tests** covering items 17-20 (plus an explicit empty-state and single-project-no-auto-select test) through the real, full `AppShell` tree.

```
Test Files  6 passed (6)
     Tests  86 passed (86)
```
(14 `aiqaApi.test.ts` + 11 `AuthContext.test.tsx` + 14 `ProjectContext.test.tsx` + 21 `StateViews.test.tsx` + 6 `ProjectSelector.test.tsx` + 20 `AppRoutes.test.tsx`.)

Item 22 (existing FE-1/2/3/4 tests remain passing) is satisfied by that same full-suite run — none of the 46 pre-existing tests were modified, weakened, or skipped.

One real bug was found and fixed while writing these tests: the first draft's `fetch` mocks used `mockResolvedValue(response)`, which returns the *same* `Response` object for every call — since a real authenticated transition causes **two** real calls (`AuthContext.validate()`'s own probe, then `ProjectContext`'s own fetch), the second call's `res.json()` threw `"Body is unusable: Body has already been read"`. Fixed by switching every such mock to `mockImplementation(() => jsonResponse(...))`, which constructs a fresh `Response` per call — a test-harness bug, not a `ProjectContext` bug, but disclosed here since it initially produced 16 failing tests before diagnosis.

## 15. Manual verification

Ran the real Vite dev server against a **real, running `services/orchestrator`** backend, driven through the in-app browser — two real projects ("AIQA Core Platform", "Payments Gateway QA") were created via the real `POST /api/v1/projects` endpoint for this session (no direct database writes were used to manufacture this result — see §19).

- **Scenario A (no key)** — Header showed "No project scope"; network log confirmed no `/api/v1/projects` request was ever sent. **PASS.**
- **Scenario B (valid key)** — real admin key configured and verified against the live backend; selector transitioned to "Select a project" (2 real projects loaded, correctly not auto-selected). **PASS.**
- **Scenario C (one project)** — a genuinely project-scoped key was minted via the real `POST /admin/api-keys` endpoint (not fabricated); the selector loaded exactly the one real, correctly-scoped project and still did not auto-select it. **PASS.**
- **Scenario D (multiple projects)** — with the global admin key, the dropdown opened and listed both real projects; selecting "AIQA Core Platform" updated the header globally. **PASS.**
- **Scenario E (refresh)** — after selecting a project, the dev server was restarted (see below) and the page reloaded: the key correctly returned to `key_present` (unverified, per session semantics), the header correctly showed "No project scope" until re-verified, and clicking "Verify Connection" **re-adopted the persisted project** automatically once the real fetch succeeded. **PASS.**
- **Scenario F (sign out)** — selected a project, then clicked "Sign Out" from Settings: the header immediately returned to "No project scope", and both `aiqa.orchestrator.apiKey` and `aiqa.selectedProjectId` were confirmed `null` in `localStorage`. **PASS.**
- **Scenario G (401)** — the stored key was overwritten with a well-formed-but-wrong value and verified against the real backend: real `401`, header showed "No project scope" (no fabricated authenticated data), and the persisted project id was confirmed wiped. **PASS.**
- **Scenario H (offline)** — the real orchestrator process died mid-session (see the incident note below) and "Verify Connection" was clicked against it: the real card showed "Orchestrator unreachable"; after the backend was restarted, "Retry" succeeded and **the previously-selected project was still shown** (preserved, not fabricated-fresh), matching §5/§6's offline rule. **PASS** — this scenario was exercised via a genuine, unplanned backend outage rather than a deliberately staged one, and is reported exactly as it happened.
- **Scenario I (Dashboard)** — confirmed live: KPI cards, project spec panel, execution telemetry, and the page's own internal fake selector all rendered identically to FE-4, with the real `ProjectSelector` coexisting in the header. **PASS.**
- **Scenario J (Test Generator)** — confirmed live: header, tabs, generated-cases state, and category filters all rendered identically to FE-4. **PASS.**
- **Scenario K (URL security)** — `window.location.href`/`.pathname`/`.search` inspected after every scenario above; the API key and every project id never appeared in any of them. **PASS.**

**Incident note (disclosed, not hidden):** partway through manual verification, the background `aiqa-orchestrator.exe` process from this session died on its own (environment/session churn, not something this sprint's code did), which is what produced the genuine offline state exercised in Scenario H; it was restarted (same binary, same on-disk SQLite store — no data was lost or fabricated) before continuing.

## 16. Typecheck/build

Exact commands, run from `forentend/`:

```
npx tsc --noEmit     # clean, no errors
npx vitest run       # 86/86 passing
npx vite build       # succeeds, 2709 modules, ~10s
```

The production build's >500kB chunk-size warning is pre-existing (present before FE-5 — see the FE-4 report §14) and unchanged in kind by this sprint.

## 17. Regression

All 46 pre-existing FE-1/2/3/4 tests (14 `aiqaApi.test.ts` + 11 `AuthContext.test.tsx` + 21 `StateViews.test.tsx` + 20 `AppRoutes.test.tsx`) still pass, unmodified in assertion content. `App.tsx`, `Header.tsx`, `DashboardPage.tsx`, and `TestCaseGeneratorPage.tsx` each received only the additive, documented touches described in §5, §9, §10 — no existing test needed to change because of them.

## 18. FRD/SRD/PRD/Flow compliance

Re-confirmed at the start of this sprint (same exhaustive, repeatable search as FE-4's own report §3): **no FRD/SRD/PRD/flow-diagram document exists anywhere in this repository.**

1. **Project selection rules found:** none, documented. Resolved via the real backend contract (`list_projects_handler` already filters to the caller's accessible set) plus this sprint's own explicit cautions (§5, §6, §9).
2. **Project persistence rules found:** none, documented. Resolved via the smallest-safe-behavior rule (§6): persist only an opaque id, in a separate storage key, always re-validated against the real backend before being trusted.
3. **Project switching behavior found:** none, documented. Resolved via the brief's own explicit constraints (§14): no unrelated auth change, no full app reload, stale local state cleared via a remount key on the two pages that have any.
4. **Project authorization/scoping requirements found:** the real backend contract itself is authoritative here (project-scoped API keys, `list_projects` filtering, 403 on cross-project/role-insufficient requests) — this context only mirrors what the backend already enforces, never re-implements or second-guesses it.
5. **Missing documentation:** all of the above — no FRD/SRD/PRD/flow diagrams exist to consult.
6. **Contradictions:** none found between this sprint's brief and the real backend contract; the one internal tension in the brief itself (Case B's "possible... but do not assume" phrasing) was resolved conservatively and is disclosed in §5, not silently picked.
7. **Behavior that could not be verified from documentation/backend contract:** whether a lone accessible project *should* auto-select (brief explicitly anticipates this is undocumented) — resolved as "no", disclosed as a judgment call, trivially reversible (one `if` in `applyList`) if product documentation later specifies otherwise.

## 19. Backend changes

**None.** No file under `services/orchestrator/src` or `migrations/` was created, edited, or deleted this sprint. The two real projects and the one project-scoped API key used for manual verification (§15) were created through the orchestrator's own existing, real, authenticated REST endpoints (`POST /api/v1/projects`, `POST /admin/api-keys`) — the same way any real client would — not by writing to the SQLite database directly. (This differs from FE-4's session, which *did* need a direct-DB key insert because no backend API existed yet to mint the very first key without already holding one; by FE-5, a real admin key from that earlier session already existed, so every credential and project used here came through genuine backend endpoints.)

## 20. Limitations

- Auto-selection for a single accessible project is deliberately not implemented (§5, §18) — a defensible, disclosed, easily-reversible reading of an intentionally ambiguous brief, not a discovered bug.
- Dashboard and Test Generator's own visible "current project" concept remains 100% `AIQAContext` fake data, coexisting with (not replaced by) the new real global scope — real per-project data wiring for either page is out of scope until their real backend data needs are addressed (later FE units).
- `ProjectContext`'s own `error` state can, in a genuine race (key revoked between `AuthContext`'s last check and `ProjectContext`'s own fetch), diverge briefly from what `AuthContext.status` still reports; `ProjectContext` does not reach into `AuthContext` to force a resync (would risk becoming a second authentication mechanism) — it only reports what its own request observed.
- Scenario H's offline state was exercised via a real, *unplanned* backend process death rather than a deliberately controlled one — reported exactly as it happened rather than staged, per this sprint's own "never claim a manual PASS without genuine verification" rule.

## 21. Out-of-scope

FE-6 and onward were not started. This sprint implemented project-scope architecture only: no real per-project KPI/execution/findings/evidence data wiring, no visual redesign of any Google-Studio-generated page, and no new backend authorization logic.
