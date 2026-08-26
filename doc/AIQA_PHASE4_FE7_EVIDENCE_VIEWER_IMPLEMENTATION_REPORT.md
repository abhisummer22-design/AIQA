# AIQA — Phase 4 — FE-7 Implementation Report

## 1. Status

**PASS**

## 2. Exit criterion

FE-7's brief: *"establish the Evidence Viewer using real backend evidence capabilities only... real evidence when supported by the backend, honest empty state when no evidence exists, honest not-implemented/needs-review state when backend support is absent, no fabricated screenshots, files, URLs, artifacts, logs, or execution outputs."* Satisfied in full — the real backend **does** support evidence retrieval (§3), so this is not the PARTIAL/stop-condition path; a real Evidence Viewer (`/evidence`) now consumes it end-to-end: real runs, real evidence rows, real preview content (image/JSON/text), real download, honest Loading/Empty/No-Project/Error/Unauthorized/Forbidden/Offline states, and zero fabricated data anywhere.

## 3. Backend Evidence capability

Confirmed real, already-implemented, unmodified: three routes in `services/orchestrator/src/api.rs`:

- `GET /api/v1/projects/:project_id/executions/:run_id/evidence` (`list_evidence_scoped`) — list evidence for one run.
- `GET /api/v1/projects/:project_id/evidence/:evidence_id` (`get_evidence_detail_scoped`) — metadata detail (same `EvidenceRow` shape the list already returns per item).
- `GET /api/v1/projects/:project_id/evidence/:evidence_id/content` (`get_evidence_content_scoped`) — real raw bytes, real `Content-Type` header.

**Structural fact that shaped this sprint's architecture**: there is **no project-wide evidence-list endpoint**. Evidence exists only in relation to a run (`store.rs::list_evidence_for_run(run_id)`); a run exists only in relation to a project. So a run picker is a real, contract-driven necessity, not an invented feature — see §11.

Authorization (read from `require_scoped_project`/`require_resource_in_project`, both pre-existing, unread-only): project-scope mismatch → real 403; a run id that doesn't belong to the URL's project → real 404. **No role gate** on any evidence route (unlike Audit's `Role::Operator` requirement) — any authenticated key with correct project scope can read evidence.

## 4. API integration

**Zero new transport code.** All three real endpoints were already exposed by FE-1 before this sprint began:

```ts
aiqaApi.executions.evidence(projectId, runId, signal)  // list
aiqaApi.evidence.get(projectId, evidenceId, signal)     // detail (real, tested, not called by the page — see §9)
aiqaApi.evidence.content(projectId, evidenceId, signal) // raw bytes → Blob
```

`EvidencePage.tsx` also reuses the pre-existing `aiqaApi.executions.list(projectId)` (unrelated, unchanged) to populate the run picker. No direct `fetch()` anywhere in the page; every request goes through FE-1's `ApiError`-throwing transport; every request is `AbortController`-cancellable.

## 5. Evidence data contract

`types/contract.ts`'s `EvidenceRow` (pre-existing, unchanged) is the exact real shape, verified field-for-field against `store.rs::EvidenceRow`:

```ts
interface EvidenceRow {
  id: string;
  task_id: string | null;
  kind: string;
  object_key: string;
  content_type: string | null;
  size_bytes: number;
  created_at: string;
}
```

No `name`/`filename`/`url`/`status`/`metadata` exists on the real contract and none is rendered. **`object_key` is real but deliberately never rendered** — it is a storage-layer implementation detail (`"{run_id}/{file_name}"`, resolved server-side against a storage root with `..`-escape protection in `evidence.rs`), not a URL or anything actionable client-side; rendering it would mean exposing an internal storage path (brief §16/§25 item 5). Verified live and by automated test that it never appears in the DOM.

## 6. Authentication integration

No second state machine — identical two-layer pattern FE-6 established for Audit:

1. **Route-level** (unchanged FE-4 mechanism): `/evidence` is `protectedRoute: true`; `no_key`/`key_present`/`checking`/`unauthenticated`/`forbidden`/`offline`/`error` all render the real, existing `StateGate` card — `EvidencePage`'s body never mounts otherwise. Verified live (Scenarios A, G) and by automated test (items 8-9).
2. **Resource-level** (new, per-request): both the run-list fetch and the evidence-list fetch independently classify `ApiError.kind` and render a matching card via the same `StateCard`/`Cta` primitives FE-6 exported from `StateViews.tsx` — no new visual pattern.

## 7. Project-scope integration

`useProject()` (FE-5, unchanged) is the only project-selection mechanism — no second selector. `selectedProjectId` feeds directly into the real endpoints' required path segments; never a query parameter, never trusted from a URL (this app puts no project/run/evidence id in the URL at all — context-only scope, same resolved decision as FE-5/FE-6, since no documentation requires otherwise).

**Project switching** (verified live and by automated test, item 12/15): changing the selected project immediately clears the run selection, the evidence list, and any open preview, aborts any in-flight request for the old scope, and starts a fresh fetch for the new project's runs. The previous project's evidence is never visible during or after the transition — confirmed live via the network log itself showing a real `net::ERR_ABORTED` for the stale in-flight request when a project switch happened mid-fetch during manual testing.

**Backend remains authoritative, not just by convention but structurally**: a project-scoped API key's own `/projects` response (FE-5) only ever contains that key's own project, so the UI cannot even present an out-of-scope project as selectable — the real 403 this would otherwise trigger (confirmed independently via `curl`, see §17) cannot be organically reached through the browser UI as a result of this same protection. This is a security property, not a gap.

## 8. Evidence states

| State | Trigger | Rendered as |
|---|---|---|
| Loading | run-list or evidence-list fetch in flight | `StateCard` (spinner) |
| No project | `selectedProjectId` is null | `EmptyState` |
| No executions | project selected, real run list is empty | `EmptyState` — distinct copy from "no evidence" |
| Select an execution | runs exist, none picked yet | `EmptyState` |
| Empty | a run's real evidence list is genuinely `[]` | `EmptyState` — explicitly not an error |
| Error | any other `ApiError.kind` | `StateCard`, `error.message` + `request_id` if present |
| Unauthorized | real 401 on either fetch | `StateCard` |
| Forbidden | real 403 (project scope mismatch) | `StateCard`, distinct copy, never collapsed into empty |
| Offline | network failure | `StateCard` |
| NeedsReview | — | **Not implemented** — same reasoning as FE-6's `AuditPage`: no FRD/SRD/PRD and no existing design-system component defines this state's semantics for a resource page; the brief's own §10 permits `NeedsReview` "only if existing project semantics define it" — they don't. |

Five distinct empty-ish states (no-project / no-executions / select-an-execution / empty-evidence / no-search-match — the last from client-side filtering, not shown in the table above) were deliberately kept visually and textually distinct per brief §23's explicit requirement that "No project / No evidence / Backend unsupported... must never look like the same condition" — extended here to the additional real distinctions this page's two-level (run → evidence) scope actually has.

## 9. Evidence types / viewer

Only formats the real `content_type` field can actually indicate, using `previewKindFor()`:

- **image/*** — real bytes fetched once via `evidence.content`, wrapped in a real `URL.createObjectURL` object URL, rendered in an `<img>`. Verified live is not possible with a genuine PNG in this session (no image-producing backend action was safely available — see §22), but confirmed by automated test using a real fetched blob and asserting `URL.createObjectURL` was actually called with it.
- **application/json** — real bytes read via `FileReader` (see note below), pretty-printed with `JSON.stringify(JSON.parse(text), null, 2)` inside a `<pre>` (falls back to raw text if parsing fails) — plain text content, never `dangerouslySetInnerHTML`.
- **text/*** (including `text/html`) — rendered as plain, literal, escaped text in a `<pre>`. **Deliberately not rendered as HTML**: no HTML sanitizer (e.g. DOMPurify) exists anywhere in this codebase (checked before writing `EvidencePage.tsx`), and the brief explicitly forbids blindly injecting arbitrary HTML; showing HTML evidence as safe literal source text — readable, never executed — was chosen over adding a new dependency for one content type (brief: "do not over-engineer").
- **Anything else** (or no `content_type`) — "Preview unavailable", with the real fetched blob still offered for Download (the content endpoint doesn't require a "viewer" to be useful).

`FileReader.readAsText` (not `Blob.prototype.text()`) reads the fetched blob's text — broader real browser support, and the only one of the standard read methods (`.text()`, `.arrayBuffer()`, `.stream()`) that this project's own jsdom test environment actually implements (confirmed with a throwaway check before writing this file); this is the real production implementation, not a test-only shim.

Live-verified (§17, Scenario D): a real pytest-executor log (`text/plain`) rendered its exact real captured stdout, byte-for-byte matching the backend's own stored evidence.

## 10. Filters / sorting / pagination

- **Server-side**: none beyond the run/project path scope itself — the real `list_evidence_for_run` query takes no filter/sort/limit parameter at all (confirmed by reading `store.rs`).
- **Client-side**: none built for evidence rows themselves (unlike Audit, which had a real `limit` control) — the evidence list for a single run is expected to be small (one entry per task that produced output), so no filter UI was added; this is a documented decision, not an oversight.
- **Sorting**: none — the backend already returns rows `ORDER BY created_at` (ascending, oldest-first); no UI sort control exists.
- **Pagination**: none — no cursor/offset/page/limit exists on this endpoint. This is the complete, documented pagination story for evidence in this sprint.
- **Run picker**: uses `aiqaApi.executions.list`'s own real default (`limit` not passed → backend default of 50), matching existing behavior elsewhere in this app; no new pagination UI was built for the run picker either.

## 11. Download behavior

Genuinely supported and implemented: clicking "Download" (available once a preview has successfully fetched real bytes, for any content type — not only previewable ones) creates a `URL.createObjectURL` from the **real fetched `Blob`**, triggers a synthetic `<a download>` click against that blob URL, then revokes it. Never a fabricated URL, never `object_key` used as a path, never a credential in the URL — the blob itself was already fetched through the authenticated FE-1 transport before any URL is constructed. The filename is derived (not fabricated data) from real fields — `"{kind}-{id}{.ext}"`, where the extension is looked up from the real `content_type` via a small static map; browsers require some filename for a download and none is provided by the contract, so this is a client-side display convenience, not invented evidence content.

## 12. Security

- Verified live and by automated test: the raw API key never appears in the DOM, no `"bearer"`/`"authorization"` substring anywhere, no credential in the URL (`window.location.href`/`.search` clean).
- **Defense-in-depth redaction** (`redactOwnKey`, same pattern as FE-6's `AuditPage`): strips any occurrence of the *currently-configured* raw key from rendered text/JSON preview content — narrow and targeted (only this session's own known-sensitive value), not a broad heuristic that risks mangling legitimate evidence (brief §15: "do not over-redact legitimate test evidence"). Verified by automated test using a deliberately "poisoned" mock evidence body.
- `object_key` (a real, backend-returned field) is never rendered — verified live and by automated test.
- Evidence content is never written to `localStorage` — confirmed live: after loading real evidence, `Object.keys(localStorage)` contained only the two pre-existing keys (`aiqa.orchestrator.apiKey`, `aiqa.selectedProjectId`), nothing evidence-related.
- Project scope is enforced entirely by the real backend (`require_scoped_project`) — this frontend does not re-implement or second-guess that check; it only reflects the backend's own answer.
- 403 is never presented as an empty list — verified live is not possible for this specific endpoint through pure UI interaction (§7's structural note), but is verified independently via `curl` (§17) and by automated test using a mocked 403.

## 13. Fake-data separation

`EvidencePage.tsx` performs **zero reads from `AIQAContext`** — no `useAIQA` import, no fallback to any fabricated activity/evidence list. `AIQAContext.tsx` was **not modified** this sprint. A repo-wide search for existing fabricated "Evidence" data found none dedicated to evidence specifically (the closest pre-existing fake content, `ReportsPage.tsx`'s bug-report copy, lives on an unrelated page and is untouched); confirmed by automated test that no fabricated string ever appears on `/evidence`.

## 14. Dashboard preservation

Not modified this sprint. Confirmed live and by automated test (item 29) that its full fabricated KPI/telemetry content still renders identically with `/evidence` present in the route table.

## 15. Test Generator preservation

Not modified this sprint. Confirmed live and by automated test (item 30) identically to Dashboard's confirmation above.

## 16. Tests

**29 new tests**, all real:

- `forentend/src/services/aiqaApi.test.ts` — **8 new tests** (§26 API-layer items 1-7, item 1/2 combined into one test, plus a dedicated test each for `.get()` and `.content()`'s own real response parsing: real-response parsing for all three evidence functions, 401/403/offline classification, key never in URL, exact real path/query shape for all three functions in sequence).
- `forentend/src/components/pages/EvidencePage.test.tsx` — **21 new tests** (§26 items 8-30, several naturally combined: 10/11, 12/15, 13/14, 24/27, 25/26), exercised through the real `AppShell` tree, including run-picker interaction, project-switch cancellation, all seven states, real text/image/unsupported preview rendering, key redaction, and `object_key` non-exposure.

```
Test Files  8 passed (8)
     Tests  139 passed (139)
```
Full breakdown: `aiqaApi.test.ts` 28 (14 FE-1/2 + 6 FE-6 audit + 8 FE-7 evidence) + `AuthContext.test.tsx` 11 + `ProjectContext.test.tsx` 14 + `StateViews.test.tsx` 21 + `ProjectSelector.test.tsx` 6 + `AppRoutes.test.tsx` 20 + `AuditPage.test.tsx` 18 + `EvidencePage.test.tsx` 21 = **139**.

Item 31 (existing FE-1 through FE-6 tests remain passing) is satisfied by that same full-suite run — none of the 106 pre-existing tests were modified, weakened, or skipped.

One real, disclosed test-environment discovery, not a product bug: this project's own jsdom test environment implements `Blob` with only `.size`/`.type` — none of `.text()`, `.arrayBuffer()`, or `.stream()` exist on the `Blob` its `fetch` polyfill produces (confirmed with a throwaway diagnostic before writing any test). `FileReader.readAsText` does work correctly in this environment and is the real, shipped implementation `EvidencePage.tsx` uses for exactly this reason — not a test-only workaround. `URL.createObjectURL`/`revokeObjectURL` are similarly unimplemented by jsdom (a well-known, deliberate jsdom limitation — it has no real blob storage) and are stubbed in the test file's `beforeEach`, the same way `fetch` itself is already stubbed everywhere in this suite; this does not touch `EvidencePage.tsx`'s own logic.

## 17. Manual verification

Ran the real Vite dev server against the real `services/orchestrator` backend. **Real evidence was generated through a genuinely pre-existing, already-supported backend action** — no backend code or database was touched to manufacture it: a real execution run was created via the real `POST /api/v1/projects/:id/executions` endpoint with a task naming the backend's own real `pytest` executor (`executor: "pytest"`), which invoked the repository's own pre-existing `executors/pytest/test_health.py` against the live orchestrator's real `/health` endpoint. The backend's real engine captured the real subprocess stdout/stderr as a real evidence row (`kind: "log"`, `content_type: "text/plain"`, 970 real bytes) exactly the way it does for any real executor run — this is normal, already-implemented backend behavior, not something built for this test.

- **Scenario A (no key)** — Sidebar's "Evidence Viewer" entry appeared automatically (FE-4's route-driven navigation); opening `/evidence` showed the real "No orchestrator API key configured" card; no evidence/run request was ever sent. **PASS.**
- **Scenario B (authenticated)** — real key verified; selecting "AIQA Core Platform" showed the real run ("FE-7 evidence verification run (FAILED)") in the picker. **PASS.**
- **Scenario C (backend unsupported)** — **not applicable**: the backend genuinely supports evidence retrieval (§3), so this stop-condition path was correctly not taken.
- **Scenario D (real evidence)** — selecting the real run showed the real evidence row (`log`, `t1`, `970 B`, real timestamp); clicking it loaded the real preview, and its text matched — character for character — the real pytest failure output the backend actually produced (verified independently via `curl` against the same run beforehand). **PASS.**
- **Scenario E (project switch)** — switching to "Payments Gateway QA" immediately showed its own real, honest "No executions yet" state; the network log captured a genuine `net::ERR_ABORTED` for the stale in-flight request from the previous project, confirming real cancellation, not just a UI swap. **PASS.**
- **Scenario F (refresh)** — clicking Refresh issued exactly one new real request (confirmed via the network log). **PASS.**
- **Scenario G (401)** — the existing invalid-key flow correctly showed the route-level 401 card; `EvidencePage`'s body never mounted. **PASS.**
- **Scenario H (403)** — **not reproducible through pure browser UI interaction**, and disclosed as such rather than faked: a project-scoped key's own `/projects` list only ever contains its own project (FE-5), so the UI structurally cannot present an out-of-scope project as selectable — this is a security property of the existing architecture, not a gap in this sprint. Independently confirmed via `curl` using the exact same real endpoints this page calls: a project-A-scoped key against project B's real `/executions` and `/evidence/:id` endpoints both returned a genuine `403`. The automated test suite additionally proves the client-side rendering of a 403 (mocked at the transport boundary) is correct and distinct from empty.
- **Scenario I (offline)** — the real orchestrator process was stopped; clicking Refresh showed "Orchestrator unreachable"; after restarting the backend, Retry recovered the real data. **PASS.**
- **Scenario J (security)** — after loading real evidence and its real preview, `window.location.href`/`.search`, the full rendered DOM, and `Object.keys(localStorage)` were all inspected: no raw key, no `"bearer"`/`"authorization"` substring, no `object_key` value, and no evidence content in `localStorage`. **PASS.**

No scenario above is claimed without having genuinely been exercised; Scenario H's UI limitation is disclosed plainly rather than worked around with a fabricated result.

## 18. Typecheck/build

Exact commands, run from `forentend/`:

```
npx tsc --noEmit     # clean, no errors
npx vitest run       # 139/139 passing
npx vite build       # succeeds, 2711 modules, ~12s
```

The production build's >500kB chunk-size warning is pre-existing (present since before FE-4) and unchanged in kind by this sprint — not a new FE-7 regression.

## 19. Regression

All 106 pre-existing tests (FE-1 through FE-6) still pass, unmodified in assertion content. `routeConfig.tsx` and `types.ts` each received an additive route/union-member entry; the `designStatus` type's third bucket (introduced FE-6 as `fe6-new-follows-approved-language`) was renamed sprint-agnostically to `new-follows-approved-language` so FE-7 could reuse it honestly without a stale FE-6-specific name — `AuditPage`'s own route entry was updated to match, a one-line, non-behavioral change. No existing page, provider, route, or test assertion was altered in behavior.

## 20. FRD/SRD/PRD/Flow compliance

Re-checked at the start of this sprint (same exhaustive search as every prior FE unit): **FRD/SRD/PRD/flow-diagram source files were not present in the repository at FE-7 implementation time.**

- **Backend-derived behavior**: the entire data contract (§5), the run-scoped-not-project-scoped list structure that drove the run-picker architecture (§3, §11), the real authorization rules (§7, §12), and the absence of any filter/sort/pagination parameter (§10) all came directly from reading `services/orchestrator/src/api.rs`/`store.rs`/`evidence.rs`.
- **Frontend-contract-derived behavior**: `EvidenceRow`, `aiqaApi.evidence.*`, `aiqaApi.executions.evidence`/`.list` (all pre-existing from FE-1, zero changes) supplied the exact real shapes; no new type was invented.
- **FE-7's own explicit constraints**: drove every "do not implement" decision — no NeedsReview semantics, no HTML rendering without an existing sanitizer, no fabricated filename data presented as real evidence, no invented pagination.
- **Unresolved product decisions, disclosed rather than silently picked**: whether evidence should ever support a dedicated detail route vs. inline preview (resolved: inline, smallest UI matching the flat data shape — brief §17 explicitly permits this); whether a run should auto-select (resolved: no, consistent with FE-5's own precedent for projects, for the same undocumented-so-don't-assume reason).

No contradiction was found between this sprint's brief and the real backend contract.

## 21. Backend changes

**None.** No file under `services/orchestrator/src` or `migrations/` was created, edited, or deleted this sprint. The one real evidence artifact used for manual verification (§17) was produced entirely through the orchestrator's own existing, real, authenticated `POST /api/v1/projects/:id/executions` endpoint, naming the backend's own pre-existing `pytest` executor and its own pre-existing `executors/pytest/test_health.py` file — normal, already-supported execution behavior, not a new capability, a direct database write, or a fabricated file.

## 22. Limitations

- Scenario H (403) could not be reproduced through pure browser-UI interaction — a direct consequence of FE-5's own correct project-scope enforcement (a scoped key's `/projects` list never contains an out-of-scope project), not a gap in this sprint. Independently confirmed via `curl` against the real endpoints instead (§17).
- No real image (`image/*`) evidence could be safely produced through an already-supported backend action within this session (the available real executors — pytest/playwright/newman/maestro — did not have a trivially-triggerable image-producing path without deeper environment setup); the image preview path is verified by automated test using a real fetched blob and a real `URL.createObjectURL` call, but not against a genuine backend-produced image file. Disclosed rather than staged as a false "live PASS."
- Evidence resource-level 401/403/offline/error states are exercised independently of the route-level `AuthContext` states; in a genuine race (key revoked between the two checks), the two could theoretically show different information for a moment — the same class of limitation already disclosed for `ProjectContext` (FE-5) and `AuditPage` (FE-6), not new to this sprint.

## 23. Out-of-scope

FE-8 and onward were not started. This sprint implemented the Evidence Viewer's architecture and real data wiring only: no realtime/streaming evidence updates (explicitly deferred — "Realtime belongs to FE-9"), no visual redesign of any other Google-Studio-generated page, and no new backend authorization or storage logic.
