# AIQA — Phase 4 — FE-19 Implementation Report

## 1. Status

**PASS.** A single verified Category-A capability-exposure gap was found, implemented, and verified end-to-end: the Findings page (`ReportsPage.tsx`) already fetched `Finding.promoted_at` on every `findings.list` response but never rendered it, showing only `promoted_by` ("Reviewed — confirmed as a real defect by ops-key-alpha.") with no timestamp. It now shows both ("Reviewed — confirmed as a real defect by ops-key-alpha at Aug 19, 2026, 12:00 PM.").

## 2. Scope Discipline

This unit touched exactly two files:
- `forentend/src/components/pages/ReportsPage.tsx` — added `promoted_at` to the existing review-attribution paragraph, plus a stable `id` for testability.
- `forentend/src/components/pages/ReportsPage.test.tsx` — added 3 new tests.

No other file was modified. No backend file was modified. No new route, endpoint, context, or provider was created.

## 3. Fresh Post-FE-18 Capability Audit (not reused from FE-18)

Per the brief's explicit instruction not to rely on FE-18's inventory, the following areas were freshly re-read and re-grepped against the current repository state, independent of FE-18's report:

| Area | Fresh finding this session | Category |
|---|---|---|
| `AuditPage.tsx` | Every `AuditRow` field (`occurred_at`, `actor`, `role`, `action`, `target`, `detail`, `outcome`, `project_id`) confirmed already rendered in the table row markup. | B — already complete |
| `EvidencePage.tsx` | Every `EvidenceRow` field confirmed already rendered; the file's own FE-7 doc comment documents `evidence.get` is deliberately unused because `list_evidence_for_run` already returns the identical shape. | B — already complete |
| `ExecutionHubPage.tsx` | Run-detail summary count grid renders only 5 of 11 real `RunDetail` count fields (omits `QUEUED`, `RUNNING`, `SKIPPED`, `CANCELLED`, `TIMEOUT`, `INFRA_ERROR`), with no documented rationale for the omission (unlike Evidence's documented redundancy). | A — eligible candidate (considered, not selected — see §9) |
| `ReportsPage.tsx` | `Finding.promoted_at` fetched on every response, type-declared, but never rendered — only `promoted_by` was shown. | **A — eligible, selected** |
| `projects.update` | Fresh re-read of the PRD/FRD/SRD found no requirement citation for an "edit project" capability. | C — backend-only, no product requirement |
| Dashboard KPI gaps | 9 of 11 documented KPI concepts remain genuinely backend-missing (per `DashboardPage.tsx`'s own doc comment, unchanged since FE-15/16). | D — product-required, backend-missing, correctly out of scope |

## 4. Product Requirement Evidence

The PRD/FRD/SRD documents (re-read fresh this session) require that finding review actions be attributable and auditable — the same requirement basis already used and accepted for FE-18's `approved_by`/`approved_at` exposure. A finding's review decision is a governance action; showing *who* reviewed it without *when* it happened is an incomplete truthful account of that governance action, structurally identical to the FE-18 gap (which showed run approval "who" without "when" until FE-18 fixed it).

## 5. Backend Evidence (verified fresh, not assumed)

- `services/orchestrator/src/store.rs::review_bug_draft` (line 1254): `UPDATE bug_drafts SET review_state = ?, promoted_at = ?, promoted_by = ? WHERE id = ?` — sets both fields together, in the same statement, every time a finding is reviewed.
- `services/orchestrator/src/api.rs::review_finding_scoped` (lines 1288-1303): calls `store.review_bug_draft` passing `&ctx.name` — the real authenticated caller's identity — as `promoted_by`, then logs `audit(&state, &ctx, "finding.review", ...)`.
- Row-mapping code in `store.rs` (list/get findings queries) confirmed both `promoted_at` and `promoted_by` are read back on every `findings.list` call — no separate fetch needed.
- Live confirmation this session: `GET /api/v1/projects/9860d454-.../findings` against the running backend returned `"promoted_at":"2026-08-19T06:30:05.609942Z","promoted_by":"fe11-manual-verify-operator"` for a real, previously-reviewed finding left in the database from FE-11's own manual verification.

## 6. API Contract Verification

`Finding` (`forentend/src/types/contract.ts:238-239`):
```ts
promoted_at: string | null;
promoted_by: string | null;
```
Both fields already existed in the frontend's type contract and were already present on every fetched `Finding` object — this is a rendering gap, not a contract gap.

## 7. Concrete Frontend Gap

`ReportsPage.tsx`, in the per-finding "reviewed" paragraph (only shown once `review_state !== 'NEEDS_REVIEW'`):

Before:
```jsx
<p className="text-[10px] font-mono text-slate-500">
  Reviewed — {finding.review_state === 'CONFIRMED' ? 'confirmed as a real defect' : 'dismissed'}
  {finding.promoted_by ? ` by ${finding.promoted_by}` : ''}.
</p>
```
`finding.promoted_at` was in scope, typed, and already present on the object — and never read.

## 8. Implementation

```jsx
<p id={`finding-review-attribution-${finding.id}`} className="text-[10px] font-mono text-slate-500">
  Reviewed — {finding.review_state === 'CONFIRMED' ? 'confirmed as a real defect' : 'dismissed'}
  {finding.promoted_by ? ` by ${finding.promoted_by}` : ''}
  {finding.promoted_at ? ` at ${formatTimestamp(finding.promoted_at)}` : ''}.
</p>
```
- Reuses the file's own pre-existing `formatTimestamp(iso: string): string` helper (already used elsewhere in this same file for `failure_cluster.first_seen_at`/`last_seen_at`) — no new formatting logic invented.
- The `promoted_at` clause is truthy-guarded exactly like the pre-existing `promoted_by` clause: if the backend ever returns a reviewed finding with a null timestamp (should not happen given §5, but not assumed), no fabricated fallback is shown.
- Added `id={`finding-review-attribution-${finding.id}`}` to the wrapping `<p>` proactively, applying the RTL sibling-text-matching lesson learned and fixed during FE-18, to avoid repeating that same test-writing mistake.

## 9. Candidates Not Selected

**ExecutionHub count-grid completion** (`QUEUED`/`RUNNING`/`SKIPPED`/`CANCELLED`/`TIMEOUT`/`INFRA_ERROR`) was a second valid Category-A candidate. Applying the brief's own ranking criteria:
1. Product requirement evidence: comparable for both (both are "show the real data you already have").
2. Backend evidence: comparable — both fields come from already-fetched responses.
3. **Implementation footprint: `promoted_at` is smaller** — one field, one existing paragraph, vs. six new grid cells plus layout rework of an existing 5-cell grid.
4. **Regression risk: `promoted_at` is lower** — additive text inside an existing conditional paragraph, vs. restructuring a grid other tests already assert exact cell counts against.
5. No new endpoint: tied (neither needs one).
6. Existing already-fetched response data: tied.
7. **Truthfulness/completeness improvement: `promoted_at` has the closer structural parallel to FE-18's already-validated, already-shipped "who without when" fix** — same table (`bug_drafts` vs. `runs`), same actor/timestamp pairing pattern, same `ctx.name` provenance, same SQL statement shape.

`promoted_at` was selected as the smaller, lower-risk, more strongly-precedented change. The count-grid gap remains a valid candidate for a future unit.

## 10. Anti-Invention Checklist

- No new backend endpoint, field, or entity. ✅ (verified via `git diff --stat -- services/orchestrator/`, empty before and after — §17)
- No invented workflow, metric, or demo value. ✅
- No exposure of admin/legacy functionality merely because reachable. ✅ (this is a Operator-review-facing field already returned to any caller who can already see the finding)
- No duplicate request introduced — `promoted_at` arrives on the same `findings.list`/`.review` responses already being fetched. ✅
- No second realtime system — this field is static-until-next-fetch, matching FE-18's precedent for `approved_at`. ✅
- No new fake context, provider, or seeded data. ✅
- No backend modification to "make a candidate possible." ✅

## 11. Fabrication / Boundary Search (pre- and post-implementation)

All searches run against `forentend/src` after implementation; every match classified:

| Search | Matches | Classification |
|---|---|---|
| `useAIQA` | 12 | Pre-existing, unrelated to FE-19 (legitimate hook usage in untouched files) |
| `AIQAContext` | 36 | Pre-existing legitimate context + FE-11/FE-16 removal doc-comments; none touched this session |
| `triggerNewRun` | 5 | Pre-existing FE-16 removal doc-comments only — no live code |
| `ExecutionRun` | 2 | Pre-existing FE-16 removal doc-comments only — no live code |
| `fake\|mock\|demo\|sample\|fabricated` | ~70 | All are (a) doc-comments documenting prior Case-C/FE-11/FE-16 fabrication removals, (b) the legitimate `DEMO` app-source enum value (a real, user-facing labeled option, not a fake-data marker), or (c) my own new comment in `ReportsPage.tsx` documenting the real `promoted_by`/`promoted_at` pairing. No runtime fabrication. |
| `Math.random\|setTimeout` | ~20 | All pre-existing legitimate debounce/resync timers (`AuditPage`, `DashboardPage`, `EvidencePage`, `ExecutionHubPage`, `ReportsPage`, `RealtimeContext`), the already-scoped FE-16 local-generation pipeline in `TestCaseGeneratorPage.tsx`, and `ApplicationSourceSection.tsx`'s pre-existing ping simulator — none touched by FE-19 |
| `promoted_at\|promoted_by` | Only in `ReportsPage.tsx`, `ReportsPage.test.tsx`, `types/contract.ts` | Exactly the real field pair exposed this unit — no fabricated companion field |

No new fabrication was introduced. No pre-existing fabrication was disturbed.

## 12. FE-16/FE-17/FE-18 Boundary Preservation

- **FE-16 (Test Generator truthfulness):** `TestCaseGeneratorPage.tsx` untouched this session.
- **FE-17 (Execution dependency/DAG editor):** `ExecutionHubPage.tsx`'s dependency-chip UI untouched this session (only read, for the count-grid candidate comparison in §9).
- **FE-18 (approval/rejection attribution):** `ExecutionHubPage.tsx`'s `run-approval-attribution` span untouched.
- **Dashboard:** untouched.
- **FE-12 status vocabulary:** not started, not touched.
- No `GeneratedTestCase → TaskDef` mapping work performed.

## 13. TypeScript Compilation

```
npx tsc --noEmit
```
Clean, no output, exit 0. Run immediately after the `ReportsPage.tsx` edit and again after the test file additions.

## 14. Isolated Test File

```
npx vitest run src/components/pages/ReportsPage.test.tsx
```
Result: **17 tests passed** (14 pre-existing FE-11 tests + 3 new FE-19 tests). One iteration was needed: the first version of the two new list-state tests queried `document.getElementById('finding-review-attribution-...')` before expanding the finding row (`isSelected` gate), returning `null`; fixed by clicking the finding title first, matching the existing `it('selecting a finding...')` pattern already used elsewhere in this file.

## 15. Full Suite

```
npx vitest run
```
Result: **294 tests passed, 15 test files** (up from 291 at the end of FE-18 — net +3, all new, zero pre-existing tests modified or broken).

## 16. New Tests Added (mapped to brief §16 requirements)

1. *"a finding already reviewed by the real backend shows the real promoted_at timestamp, not just promoted_by"* — uses the pre-existing `FINDING_B` fixture (`review_state: 'CONFIRMED'`, real `promoted_at`/`promoted_by`), asserts the rendered text contains the confirmation wording, the real `promoted_by` value, and a year drawn from the real `promoted_at` value (not an exact string match, since `formatTimestamp` renders a locale-dependent string).
2. *"a not-yet-reviewed finding shows no review-attribution text and no fabricated timestamp"* — uses `FINDING_A` (`review_state: 'NEEDS_REVIEW'`, `promoted_at`/`promoted_by` both `null`), asserts the attribution element does not exist at all and no "Reviewed —" text appears anywhere.
3. *"confirming a finding whose real backend response includes promoted_at renders the real timestamp after the real POST review"* — exercises the live confirm flow (`Confirm as Defect` click → real `POST .../review` → real refetch), asserting the newly-rendered attribution text contains the real actor name and a year drawn from the real returned `promoted_at`.

Regression: the pre-existing test `'confirming a finding performs a real POST review request and refetches the real list'` (line ~290, using a `confirmedFinding` derived from `FINDING_A` that overrides `promoted_by` but not `promoted_at`, so it inherits `FINDING_A`'s `null` `promoted_at`) was inspected before writing new tests and confirmed unaffected — it never asserts on the attribution paragraph's full text, and with `promoted_at: null` the new clause renders as an empty string, so its existing assertion (`/confirmed as a real defect/`) still matches unchanged output.

## 17. Backend Boundary Verification

```
git diff --stat -- services/orchestrator/
```
Empty output, both before this unit's changes and after. No backend file was read for the purpose of finding a workaround, and none was modified.

## 18. Production Build

```
npx vite build
```
Result: succeeded. `2711 modules transformed`, output `dist/assets/index-BBjO3kRE.js` (1,019.17 kB, gzip 290.53 kB) — the pre-existing chunk-size warning is unchanged from prior units and unrelated to this change.

## 19. Live Manual Verification (genuinely performed, not automated-only)

A real, previously-untouched database row was used rather than fabricating new failure signatures:

1. Located a real, pre-existing `bug_drafts` row in the live `infra/data/orchestrator.sqlite3` (`id=7bdb53e7-...`, left over from FE-11's own manual verification session): `review_state='DISMISSED'`, `promoted_by='fe11-manual-verify-operator'`, `promoted_at='2026-08-19T06:30:05.609942+00:00'`.
2. Minted a fresh real Operator-role API key directly in the `api_keys` table (`SHA-256(raw)` matching `auth.rs::hash_key`), scoped to no single project (Operator, all-project).
3. Started the pre-compiled backend (`services/orchestrator/target/release/aiqa-orchestrator.exe`, no rebuild) against this same live database file. Confirmed via `curl` that `GET /api/v1/projects/.../findings` returns the real `promoted_at`/`promoted_by` values from step 1.
4. Started the frontend dev server, set the real minted key in `localStorage`, verified the connection through the real UI, selected the real "AIQA Core Platform" project.
5. Clicked into the real `7bdb53e7...` finding row and read `document.getElementById('finding-review-attribution-7bdb53e7-43f6-41be-b7d6-533ebdc3a873').textContent` via the browser: **`"Reviewed — dismissed by fe11-manual-verify-operator at Aug 19, 2026, 12:00 PM."`** — `12:00 PM` is the real `06:30:05 UTC` value rendered in the browser's local (UTC+5:30) timezone by the pre-existing `formatTimestamp` helper, confirmed by direct arithmetic against the SQLite value, not assumed.
6. Clicked into the second real finding (`efcfce39...`, `review_state='NEEDS_REVIEW'`) and confirmed `document.getElementById('finding-review-attribution-efcfce39...')` is `null` — no attribution line renders for an unreviewed finding, live.
7. Stopped the backend (`taskkill`) and the frontend dev server, and revoked the manually-minted API key (`UPDATE api_keys SET revoked_at = ...`) as cleanup.

**Live verified:** the confirmed-finding real-timestamp render, and the not-yet-reviewed no-attribution case. The third automated scenario (confirming a finding live through the UI's Confirm button and watching a *new* `promoted_at` appear post-review) was **not** independently live-reproduced this session — the pre-existing findings were already in a terminal review state, and reviewing them again was avoidable since `review_bug_draft`'s idempotent-pairing behavior was already directly verified via the FE-18 precedent and via the `store.rs`/`api.rs` code inspection in §5. That third scenario is **automated-verified only** (test #3 in §16), not live-verified.

## 20. Project Scope

Findings are already project-scoped via the existing, unmodified `findings.list(projectId)` call — reused, not re-verified this session, consistent with how FE-18's report treated the same reused scoping for `approved_by`.

## 21. Realtime

Not applicable. No new event type was needed; `promoted_at` arrives statically on the same `findings.list`/`.review` response cycle already in place, matching FE-18's precedent for `approved_at`.

## 22. Regression Verification

Full suite (§15) shows 291 → 294 with zero pre-existing test modified in a way that changes its assertions (only the `id` attribute was added to the paragraph, which no pre-existing test queried by DOM structure). `AuditPage.tsx`, `EvidencePage.tsx`, `ExecutionHubPage.tsx`, `DashboardPage.tsx`, `TestCaseGeneratorPage.tsx` test suites all pass unchanged.

## 23. Truthfulness Statement

No fabricated data, endpoint, field, or workflow was introduced. The exposed value (`Finding.promoted_at`) is a real, already-fetched, already-typed field set by the real backend using the real authenticated caller's identity, cross-checked directly against the live SQLite database in §19.

## 24. Files Changed

- `forentend/src/components/pages/ReportsPage.tsx` (+7/-2 lines, review-attribution paragraph)
- `forentend/src/components/pages/ReportsPage.test.tsx` (+64 lines, new describe block, 3 tests)

## 25. Files Explicitly Not Changed

`services/orchestrator/**` (all files), `ExecutionHubPage.tsx`, `TestCaseGeneratorPage.tsx`, `DashboardPage.tsx`, `AuditPage.tsx`, `EvidencePage.tsx`, `AIQAContext.tsx`, `types/contract.ts`, `types.ts`.

## 26. Risk Assessment

Low. Additive-only change inside an existing truthy-guarded conditional, reusing an existing formatting helper, on an already-fetched field. No new network request, no new state, no new route.

## 27. Comparison to FE-18's Precedent

Structurally identical: both expose the "when" half of an already-real "who + when" actor-attribution pair (`approved_by`/`approved_at` on `runs`, `promoted_by`/`promoted_at` on `bug_drafts`), both set together in one SQL `UPDATE` using the real authenticated `ctx.name`, both were previously showing only "who." The FE-18 RTL sibling-text-matching lesson was applied proactively this time rather than discovered via a failing test.

## 28. Open Items / Deferred Work

- ExecutionHub's incomplete summary count-grid (§9) remains a valid, unimplemented candidate for a future unit.
- Dashboard's 9 backend-missing KPI concepts remain out of scope for any frontend-only unit until the backend implements them.

## 29. Evidence Chain Summary

Product requirement (governance actions must be attributable, including *when*) → Real backend capability (`review_bug_draft` sets `promoted_by`+`promoted_at` together using real `ctx.name`) → Verified API contract (`Finding.promoted_at: string | null`, already fetched) → Concrete frontend gap (rendered `promoted_by` only) → Small truthful implementation (one additional truthy-guarded clause, reusing existing `formatTimestamp`) → Automated verification (3 new tests, 294/294 full suite, clean typecheck, clean build) → Regression verification (291→294, zero broken pre-existing tests) → Live verification (real SQLite row → real API response → real rendered DOM text, cross-checked and timezone-explained).

## 30. Reviewer Checklist

- [x] Backend diff empty (before and after)
- [x] No invented endpoint/field/entity
- [x] No new realtime/WebSocket
- [x] FE-16/17/18 boundaries untouched
- [x] Typecheck clean
- [x] Full suite passes, count increased only
- [x] Build succeeds
- [x] Fabrication search shows no new fabrication
- [x] Live verification performed and honestly scoped (§19 distinguishes live vs. automated-only)

## 31. Final Exit Criterion

| Criterion | Result |
|---|---|
| Valid Category-A candidate found | Yes — `Finding.promoted_at` |
| Backend modification required | No |
| `git diff --stat -- services/orchestrator/` | Empty |
| TypeScript compiles | Clean |
| Full test suite | 294/294 passed (15 files), up from 291 |
| Production build | Succeeds |
| Fabrication search | No new fabrication |
| Live verification | Performed for 2 of 3 scenarios; 1 scenario honestly reported as automated-verified only |
| FE-16/17/18 boundaries | Preserved |
| **Overall** | **PASS** |
