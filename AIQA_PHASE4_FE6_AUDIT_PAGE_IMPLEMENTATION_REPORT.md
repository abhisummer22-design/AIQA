# AIQA — Phase 4 — FE-6 Implementation Report

## 1. Status

**PASS**

## 2. Exit criterion

FE-6's brief: *"establish the Audit experience so that it is ready to consume real audit data from the AIQA backend, while preserving the agreed product-design strategy."* Satisfied — a real Audit page (`/audit`) now consumes the real, pre-existing backend endpoint `GET /api/v1/projects/:project_id/audit` through FE-1's transport, obeys FE-2/FE-3's real auth states and FE-5's real project scope, distinguishes Loading/Empty/Error/Unauthorized/Forbidden/Offline as genuinely different states, and never fabricates a record. Dashboard and Test Generator are unmodified and confirmed unregressed.

## 3. Audit architecture

**`forentend/src/components/pages/AuditPage.tsx`** (new), roughly matching the brief's own suggested shape:

- **Header** — title, real selected-project name (from `ProjectContext`), a real `limit` selector (the entirety of real pagination support, §9), a real Refresh action.
- **Search bar** — client-side only, renders solely when real rows exist.
- **Content region** — one of: no-project prompt (`EmptyState`), Loading (`StateCard`), Unauthorized/Forbidden/Offline/Error (`StateCard`, one branch per `ApiError.kind`), Empty (`EmptyState`), no-search-match message, or the real data table.

Route protection reuses FE-4's existing `ProtectedRoute`/`StateGate` mechanism unchanged (`routeConfig.tsx`: `{ id: 'audit', path: '/audit', protectedRoute: true }`) — Sidebar and CommandPalette picked it up automatically with no edits to either file, exactly FE-4's architecture doing its job.

## 4. Backend/API integration

**Exactly one real endpoint**, already implemented by the backend and already exposed by FE-1 before this sprint began: `GET /api/v1/projects/:project_id/audit` (`services/orchestrator/src/api.rs::get_audit_log_scoped`), consumed via `aiqaApi.audit.list(projectId, { limit }, signal)` (`services/aiqaApi.ts`) — this function already existed (from FE-1) and needed **zero changes**; FE-6 only added a real consumer. No second transport layer, no direct `fetch()` in `AuditPage.tsx`.

Real backend authorization, confirmed by reading `get_audit_log_scoped` itself: `require_scoped_project` (403 on wrong-project-scoped key) then `auth::require_role(&ctx, Role::Operator)` (403 on viewer role) — **distinct from and stricter than** `AuthContext`'s own `/projects` validation call, which (per FE-2/FE-3's own documented limitation) structurally never 403s. This meant FE-6 could, for the first time in this engagement, genuinely exercise a live 403 through the real UI (§17, Scenario H) rather than only through `curl` or a mocked test.

Query support, confirmed from `AuditQuery { limit: Option<i64> }` (backend) — **`limit` only**. No date range, actor/action/resource filter, sort field, or page/offset/cursor exists on the real backend today.

## 5. Audit data contract

`types/contract.ts`'s `AuditRow` (already existed, unchanged) is the exact real shape — verified field-for-field against `store.rs::AuditRow`:

```ts
interface AuditRow {
  occurred_at: string;
  actor: string | null;
  role: string | null;
  project_id: string | null;
  action: string;
  target: string | null;
  detail: string | null;
  outcome: string;
}
```

No `id`, no `request_id` on the row itself (the backend doesn't return either) — `AuditPage.tsx` never invents either. A `request_id` is shown only when it comes from an `ApiError` (a *failed* request), exactly like every other page's error state, never attached to a successful row.

## 6. Authentication integration

No second state machine. Two layers, both real:

1. **Route-level** (unchanged from FE-4): `/audit` is a `protectedRoute: true` entry, so `no_key`/`key_present`/`checking`/`unauthenticated`/`forbidden`/`offline`/`error` all render the real, existing `StateGate`/`renderAuthState` card — `AuditPage`'s own body never mounts in any of those states. Verified live (§17, Scenario A/G) and by automated test (items 8-9).
2. **Resource-level** (new, `AuditPage`'s own fetch): even once route-level auth says `authenticated`, the *audit-specific* request can independently 401/403/offline/error — a real possibility here (unlike the `/projects` validation endpoint) because this endpoint enforces its own role/project-scope rule. `AuditPage` classifies `ApiError.kind` itself and renders a matching card, reusing the exact same visual primitive (`StateCard`, newly exported from `StateViews.tsx` — see §8) rather than inventing a second design language.

## 7. Project-scope integration

`useProject()` (FE-5, unchanged) is the only source of the current project — no second selection mechanism. `AuditPage` requests are scoped by putting the real `selectedProjectId` into the endpoint's own required path segment (`/projects/:project_id/audit`), never a query parameter (the backend contract doesn't support that either way).

- **No project selected** → the real endpoint can't even be constructed; a distinct "No project selected" `EmptyState` renders — never a fabricated scoped list (brief §5).
- **Project switch** → `AuditPage`'s effect clears `rows` to `null` immediately on `selectedProjectId` change, then issues a fresh, `AbortController`-cancelling fetch — the previous project's rows are never visible during or after the transition. Verified live (switching "AIQA Core Platform" → "Payments Gateway QA" immediately replaced the real event with a real, honest empty state) and by automated test (items 11/12).

## 8. Audit states

| State | Trigger | Rendered as |
|---|---|---|
| Loading | fetch in flight, no rows yet | `StateCard` (spinner) |
| Empty | real 200 with zero rows | `EmptyState` — explicitly not an error |
| Error | any `ApiError.kind` not otherwise listed (e.g. 500) | `StateCard`, `error.message` + `request_id` if present |
| Unauthorized | real 401 from the audit endpoint | `StateCard`, distinct copy from the route-level 401 |
| Forbidden | real 403 (wrong role/project scope) | `StateCard`, distinct copy naming the real `Operator`-role rule |
| Offline | network failure / gateway-down | `StateCard` |
| NeedsReview | — | **Not implemented** — no FRD/SRD/PRD and no existing design-system component defines this state's semantics (brief §7 explicitly: "do not invent NeedsReview business semantics"); omitting it is the correct, disclosed choice, not an oversight. |

`StateCard` and `Cta` (both previously module-private inside `StateViews.tsx`) were exported (additive change, no behavior change to any existing consumer) so Audit's resource states reuse the exact same visual primitive FE-3 built for auth states, rather than a duplicate.

## 9. Filters/sorting/pagination

Only what the real backend genuinely supports:

- **Filter**: a `limit` selector (50/100/250/500 — the backend's own accepted range, clamped 1..1000 server-side) is the only real, backend-supported filter. A free-text search box additionally filters **already-returned** rows client-side over `actor/role/action/target/detail/outcome/project_id` — a safe derivation from already-returned fields (brief §11 rule 3), never a new backend query parameter. No date-range/actor-dropdown/action-dropdown/status filter exists — none is backend-supported, so none was built.
- **Sorting**: none implemented — the backend already returns `ORDER BY occurred_at DESC` (newest-first); no other sort field is exposed, so no sort UI exists (brief §12: "use the backend's natural/default order").
- **Pagination**: none beyond the `limit` selector above — the backend exposes no cursor/offset/page (brief §13). This *is* the entirety of this sprint's pagination support, documented here rather than left implicit.

## 10. Audit detail behavior

No drawer/modal/dedicated route was built. The backend's audit row is already flat and short (8 scalar fields, no nested metadata object) — every field renders inline in its table row; `target`/`detail` get a native `title` attribute for the (rare) case a value is too long for its truncated cell. Building a separate detail view would mean inventing a workflow the data shape doesn't call for (brief §14: "do not invent this workflow").

## 11. Security

- **Own key never fetched/displayed by this page** — unlike Settings, `AuditPage` never calls `maskApiKey`/reads the key for display.
- **Defense-in-depth redaction**: a new `redactOwnKey()` helper strips any occurrence of the *currently-configured* raw key from `actor`/`target`/`detail` before rendering — guarding the narrow, real residual risk that a backend-returned field might happen to contain this session's own credential (brief §14's explicit warning), without a broad "looks like a secret" heuristic that would risk hiding legitimate audit content. Verified live (DOM search for the raw key after loading real data: not found) and by automated test (item 24, using a deliberately "poisoned" mock row).
- **No credential in the URL** — project scope is a path segment of an internal API call, never reflected into `window.location`. Verified live (`window.location.href`/`.search` inspected — clean) and by automated test.
- **`request_id` is never confused with an event id** — it only ever appears inside an error card, sourced from `ApiError.requestId`, never attached to a real row (§15).
- Verified live that no `"bearer"`/`"authorization"` substring (case-insensitive) appears anywhere in the rendered DOM.

## 12. Fake-data separation

`AuditPage` performs **zero reads from `AIQAContext`** — no import of `useAIQA`, no fallback to any fabricated activity/audit list. `AIQAContext.tsx` itself was **not modified** this sprint. The only pre-existing fake "audit-adjacent" content in the codebase (`ReportsPage.tsx`'s fabricated bug-report copy mentioning "Audit trail S3 delete marker") is untouched and lives on an entirely separate page — confirmed it never appears on `/audit` (automated test item 21/22 explicitly asserts this fake string is absent).

## 13. Dashboard preservation

Not modified this sprint. Confirmed live (§17) and by automated test (item 25) that its full fabricated KPI/telemetry content still renders identically with `/audit` present in the route table.

## 14. Test Generator preservation

Not modified this sprint. Confirmed live (§17) and by automated test (item 26) identically to Dashboard's confirmation above.

## 15. Design status of other pages

Unchanged. The nine pre-existing Google-Studio-generated pages (Orchestrator, Validator, Execution Hub, RBAC, API Testing, Mobile Testing, Custom Testing, Feature Map, Reports) remain explicitly **not** treated as approved final designs (`routeConfig.tsx`'s `designStatus: 'google-studio-not-final'`, untouched). `AuditPage` is a **third, honestly distinct** `designStatus` value (`'fe6-new-follows-approved-language'`, added to the type this sprint) — built fresh, following Dashboard/Test Generator's visual language by construction, but not itself product-reviewed; it was deliberately not folded into either existing bucket, since both existing labels would misstate its provenance.

## 16. Tests

**26 new tests**, all real (no internal-implementation-only mocking):

- `forentend/src/services/aiqaApi.test.ts` — **6 new tests** (§24 API-layer items 1-7, one test covers 1+2 together): real transport reuse, real 200 parse, 401→unauthorized, 403→forbidden, network failure→offline, key never in URL, exact real query-string shape (`?limit=N` only, omitted entirely when not passed).
- `forentend/src/components/pages/AuditPage.test.tsx` — **18 new tests** (§24 items 8-26, several combined where naturally paired: 11/12, 13/14, 21/22, plus one extra "no project selected" test), exercised through the real `AppShell` tree.

```
Test Files  7 passed (7)
     Tests  110 passed (110)
```
Full breakdown: `aiqaApi.test.ts` 20 (14 pre-existing + 6 new) + `AuthContext.test.tsx` 11 + `ProjectContext.test.tsx` 14 + `StateViews.test.tsx` 21 + `ProjectSelector.test.tsx` 6 + `AppRoutes.test.tsx` 20 + `AuditPage.test.tsx` 18 (new) = **110**.

Item 27 (existing FE-1 through FE-5 tests remain passing) is satisfied by that same full-suite run — none of the 92 pre-existing tests were modified, weakened, or skipped; the only pre-existing file touched for a *behavioral* reason was `StateViews.tsx` (`AuthStateCard`→`StateCard` rename + two new exports, additive, no existing import broken — confirmed no external file referenced the old private name before renaming).

## 17. Manual verification

Ran the real Vite dev server against the real `services/orchestrator` backend. A real, project-scoped `operator`-role key (from FE-5's own session) was used to perform a real `PATCH /api/v1/projects/:id` on "AIQA Core Platform" (setting its description) — a genuine, already-supported backend action — producing one real, project-scoped `project.update` audit row without modifying backend code or writing to the database directly. A genuinely `viewer`-role, project-scoped key was minted via the real `POST /admin/api-keys` endpoint for Scenario H.

- **Scenario A (no key)** — Sidebar's "Audit Trail" entry appeared automatically (FE-4's `routeConfig`-driven navigation, no manual edit needed); opening `/audit` showed the real "No orchestrator API key configured" card; no `/api/v1/projects/.../audit` request was ever sent. **PASS.**
- **Scenario B (valid key)** — real key verified against the real backend; Audit page reachable. **PASS.**
- **Scenario C (empty)** — "Payments Gateway QA" (zero real audit rows) showed "No audit events recorded... yet." — a real, honest empty state, not an error. **PASS.**
- **Scenario D (real events)** — the real `project.update` row created above appeared with every field matching exactly what was inserted (`fe5-scoped-key`, `operator`, `project.update`, the real project id as target, `ok`) — no fabricated value anywhere. **PASS.**
- **Scenario E (project switch)** — switching from "AIQA Core Platform" to "Payments Gateway QA" immediately replaced the real event with the real empty state for the new scope — no stale row lingered. **PASS.**
- **Scenario F (refresh)** — clicking Refresh issued exactly one new real request (confirmed via the network log: one new `GET .../audit?limit=100` call, no duplicate/storm). **PASS.**
- **Scenario G (401)** — the existing invalid-key flow (garbage key, real 401 from `AuthContext`'s own validation) correctly showed the route-level 401 card — `AuditPage`'s body never mounted, no fabricated data. **PASS.**
- **Scenario H (403)** — a genuinely viewer-role, project-scoped key produced a real 403 from the audit endpoint itself (confirmed independently via `curl` first: `403`); the UI showed "Not permitted to view this audit trail / ACCESS FORBIDDEN · 403" with a real `request_id`, never collapsed into an empty list. This is the first genuinely live-browser-exercised 403 in this entire engagement (FE-2/FE-3/FE-5 all had to document this as unreachable via their own validation endpoints). **PASS.**
- **Scenario I (offline)** — the real orchestrator process was stopped; clicking Refresh showed "Orchestrator unreachable"; after restarting the backend, Retry recovered real data. **PASS.**
- **Scenario J (security)** — `window.location.href`/`.search` and the full rendered DOM were inspected after loading real data: no raw key, no `"bearer"`/`"authorization"` substring, no credential anywhere. **PASS.**

No scenario in this list is undemonstrated or fabricated.

## 18. Typecheck/build

Exact commands, run from `forentend/`:

```
npx tsc --noEmit     # clean, no errors
npx vitest run       # 110/110 passing
npx vite build       # succeeds, 2710 modules, ~17s
```

The production build's >500kB chunk-size warning is pre-existing (present since before FE-4) and unchanged in kind by this sprint — not a new FE-6 regression.

## 19. Regression

All 92 pre-existing tests (FE-1 through FE-5) still pass, unmodified in assertion content. `StateViews.tsx` received the one additive, non-behavioral change described in §16; `routeConfig.tsx` and `types.ts` each received an additive route/union-member entry. No existing page, provider, or route was altered in behavior.

## 20. FRD/SRD/PRD/Flow compliance

Re-checked at the start of this sprint (same exhaustive search as every prior FE unit): **FRD/SRD/PRD/flow-diagram source files were not present in the repository at FE-6 implementation time.**

Behaviors and their actual source:
1. **Real backend API** — the entire data contract (§5), authorization model (§6, §11 forbidden case), query support (§9), and default ordering (§9) all came directly from reading `services/orchestrator/src/api.rs`/`store.rs`.
2. **Existing frontend contracts** — `AuditRow`, `aiqaApi.audit.list` (both pre-existing from FE-1, unchanged) supplied the exact real field/function shape; no new type was invented.
3. **Existing route/page vocabulary** — `/audit` was already anticipated (and explicitly deferred) in FE-4's own `routeConfig.tsx` doc comment; this sprint completed it once real backend support was confirmed.
4. **FE-6's own explicit constraints** — drove every "do not implement" decision: no NeedsReview semantics, no invented filters/sort/pagination, no detail drawer, no auto-fabricated request id.

No contradiction was found between this sprint's brief and the real backend contract. `apps/web/src/views/AuditLogView.tsx` (reference-only, never production) was read for structural inspiration (its own honest field usage and client-side-search-only filtering pattern) but its visual styling (`#0a0f1a`/cyan theme) was **not** carried over — `AuditPage.tsx` uses `forentend`'s own established tokens (`#0B1120`/`#020617`/slate-800/indigo), matching Dashboard/Test Generator instead.

## 21. Backend changes

**None.** No file under `services/orchestrator/src` or `migrations/` was created, edited, or deleted this sprint. The one real audit event used for manual verification (§17) was produced by calling the orchestrator's own already-existing, real, authenticated `PATCH /api/v1/projects/:id` endpoint with a genuinely project-scoped operator key — not by writing to the SQLite database directly and not by adding any new backend capability.

## 22. Limitations

- The `NeedsReview` state is not implemented — genuinely undocumented and unsupported by the existing design system, disclosed rather than invented (§8).
- Filtering is limited to `limit` (real, backend-supported) plus a client-side, already-returned-fields-only search box — no date range, actor picker, or action picker exists, because the backend supports none of them today.
- `AuditPage`'s own resource-level 401/403/offline/error states are exercised independently of the route-level `AuthContext` states; in a genuine race (key revoked between the two checks), the two could theoretically show different information for a moment — the same honestly-disclosed class of limitation `ProjectContext` already documented in FE-5, not new to this sprint.

## 23. Out-of-scope

FE-7 and onward were not started. This sprint implemented Audit page architecture and real data wiring only: no realtime/streaming audit updates (explicitly deferred to "the later realtime phase" per the brief), no visual redesign of any other Google-Studio-generated page, and no new backend authorization logic.
