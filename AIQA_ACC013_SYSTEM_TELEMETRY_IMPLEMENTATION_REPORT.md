# AIQA — ACC-013 System Telemetry: Investigation and Build

**Date:** 22 August 2026
**Scope:** Build order item 2 of the confirmed sequence (`AIQA_SCOPE_RECOMMENDATIONS_REPORT.md`).
Investigate the real `/metrics` route before writing any frontend code; build only what its real data
actually supports.

---

## 1. What `/metrics` was found to contain

Read `services/orchestrator/src/metrics.rs` in full before writing a line of frontend code. It emits
exactly **six real Prometheus metrics, all pure state/status COUNT gauges or a counter — nothing else**:

| Metric | Kind | Grouped by |
|---|---|---|
| `aiqa_runs_total` | gauge | `state` (real `RunState` values) |
| `aiqa_tasks_total` | gauge | `status` (real `TaskStatus` values, including `TIMEOUT`, `FAILED`, `INFRA_ERROR`) |
| `aiqa_bug_drafts_total` | gauge | `review_state` |
| `aiqa_failure_clusters_total` | gauge | (scalar) |
| `aiqa_api_keys_active` | gauge | (scalar) |
| `aiqa_audit_log_total` | counter | (scalar) |

**There is no latency, response-time, duration, or histogram data of any kind — not even a sum or count
that could yield an average, let alone raw histogram buckets a real p95/p99 could be computed from.**
This is starker than "percentiles aren't computable from the raw buckets" — there are no buckets, no
sums, no timing fields anywhere in this endpoint. Stated plainly, per this unit's own instruction, rather
than fabricated: **p95/p99/latency/response-time are not achievable from `/metrics` at all, in any
fidelity.**

Of ACC-013's own six Acceptance-Criteria bullets (latency, response time, p95, p99, errors, timeouts):
- **`timeouts`** — real, precise match: `aiqa_tasks_total{status="TIMEOUT"}`.
- **`errors`** — real but loose match: `aiqa_tasks_total{status="FAILED"}` / `{status="INFRA_ERROR"}` are
  task-level failure counts, not HTTP-request-level error counts.
- **latency, response time, p95, p99** — not achievable from this endpoint. Not fabricated.

**A related, real duration signal does exist elsewhere** — `execution_avg_run_duration_ms`/
`execution_efficiency_ms` (BE-12b, `GET .../kpis`) — but it is project-scoped (this endpoint is not),
lives on a different route entirely, and is already displayed on the Dashboard (FE-26). Re-fetching and
re-displaying the same numbers here would be duplication, not new value, so this unit links to the
Dashboard instead of rebuilding that chart.

`/metrics` itself requires only authentication (no specific role — confirmed by reading
`get_metrics`'s handler signature) and is platform-wide, not project-scoped (its SQL has no `project_id`
filter anywhere).

---

## 2. What was built

**A genuinely new page — confirmed necessary before building, not assumed.** Unlike build order item 1,
ACC-013 was never one of FE-11's 7 Case-C stub pages; no dedicated telemetry route existed anywhere in
`routeConfig.tsx` to redirect. A new page was the only honest option.

**UI-only, with one small, justified exception.** No new backend data source, no new executor, no
backend code touched. The one non-purely-React change: `vite.config.ts`'s dev proxy gained a `/metrics`
entry (mirroring the existing `/api/v1` block exactly) — `/metrics` is a flat, legacy-style route (not
under `/api/v1/...`), so without this a dev-mode request to it would 404 against Vite's own dev server
instead of reaching the real orchestrator. This is dev-server routing configuration, not a backend change
and not a JSON-formatting adapter.

**No charting library was added.** `recharts` is a listed dependency but was unused anywhere in this app
before this unit; for six small, fully-enumerable label sets, a real, simple, proportional-width bar row
per label (real DOM elements, real text, no `ResizeObserver`/canvas dependency) was judged more honest
and more testable than introducing a charting library's runtime behavior for the first time in this
codebase for six short bar lists.

| File | Change |
|---|---|
| `forentend/src/components/pages/TelemetryPage.tsx` | **New.** Client-side Prometheus text parser (`parsePrometheusText`, exported and directly unit-tested), real fetch via `aiqaApi.metrics.get()`, real loading/error/empty states (reusing `StateCard`/`renderResourceErrorState`/`EmptyState` — no new state-UI pattern invented), bar-row visualizations for the three grouped series, scalar tiles for the three counts, and an explicit, permanent on-page disclosure of what real telemetry does *not* include, with a link to the Dashboard for real run/task duration |
| `forentend/src/components/pages/TelemetryPage.test.tsx` | **New**, 8 tests: 2 direct unit tests of the parser, 6 page tests (unauthenticated gating, real data rendering, genuine empty state, real error state, real refresh request, route non-regression) |
| `forentend/src/services/aiqaApi.ts` | `requestText` (mirrors the existing `requestBlob` pattern), `skipBase` option on `RequestOptions`/`buildUrl` (only `/metrics` needs to bypass the `/api/v1` prefix), and `aiqaApi.metrics.get()` |
| `forentend/src/routes/routeConfig.tsx` | New route registered: `/telemetry`, label "System Telemetry", `designStatus: 'new-follows-approved-language'` (same bucket as Audit/Evidence/ExecutionHub/Reports) |
| `forentend/src/types.ts` | `'telemetry'` added to `NavPage` |
| `forentend/vite.config.ts` | `/metrics` added to the dev proxy (the one non-React change, explained above) |

Sidebar and Command Palette navigation needed no separate edit — both are driven directly from
`routeConfig.tsx`'s `routeDefs` array (confirmed by reading both files before assuming a second edit was
needed).

---

## 3. Verification

```
cd forentend && npx vitest run    → 359 passed, 0 failed, 20 files (was 351/19 before this unit)
cd forentend && npx tsc --noEmit  → clean
cd forentend && npx vite build    → clean, 2716→2718 modules (re-run confirmed reproducible/identical
                                     output; one run's trailing Node/Windows process-exit libuv message
                                     was transient noise after a already-successful, exit-0 build — a
                                     clean immediate re-run produced identical output with no such
                                     message, confirming it was not a real build failure)
```

Net new tests: +8 (351→359) — 2 direct parser unit tests plus 6 page-behavior tests, all in
`TelemetryPage.test.tsx`.

---

## 4. Acceptance Matrix updated

`AIQA_ACCEPTANCE_MATRIX_v1.0.md` ACC-013: moved from all-`NOT_STARTED` to Backend/API/Test `VERIFIED`,
Frontend/Integration `TESTED`. **Final: `TESTED`, deliberately not `VERIFIED`** — per this unit's own
instruction not to let Final read as fully closed. §5A states explicitly, in full, that latency/
response-time/p95/p99 are not achievable from the real `/metrics` data and were not fabricated; only
`timeouts` (precise) and `errors` (loose) genuinely map to what ACC-013 asks for.

---

## 5. What this report deliberately did not do

- Did not fabricate a p95/p99 number, an average latency, or any timing figure — none exists in the real
  data, and none was invented. The shipped page says so in its own permanent UI copy, not just in this
  report.
- Did not add a backend JSON adapter over `/metrics` — the real text format was simple enough to parse
  client-side directly; no backend file was touched.
- Did not add a new charting library dependency for six short, real bar lists.
- Did not re-fetch or re-render the real average run/task duration data already shown on the Dashboard —
  linked to it instead, to avoid duplicating a number that already has a real home.
- Did not mark ACC-013 `VERIFIED` — real, tested telemetry now exists, but it does not close the full
  Acceptance Criteria, and the matrix says so precisely rather than implying otherwise.
