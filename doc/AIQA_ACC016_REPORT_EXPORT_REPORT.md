# AIQA — ACC-016 Report Export (PDF/Excel): Investigation and Build

**Date:** 22 August 2026
**Scope:** Build order item 3 of the confirmed sequence (`AIQA_SCOPE_RECOMMENDATIONS_REPORT.md`).
Investigate what "Bug Report" / "Test Report" / "Final Audit Report" actually resolve to before writing
any generation code; add real PDF and Excel export of the existing, real Findings/execution data.

---

## 1. What was investigated, and what was concluded, before building

Read ACC-016's exact Acceptance Criteria text (`AIQA_ACCEPTANCE_MATRIX_v1.0.md` §21) before forming any
conclusion:

> **Required Reports:** Bug Report, Test Report, Final Audit Report. **Export:** PDF, Excel. **Acceptance
> Criteria:** Reports must reconcile with durable execution state. **Verification:** Generate a full
> project report and a module-specific report; compare totals/statuses with authoritative run data.

Read `ReportsPage.tsx`'s existing data (`Finding[]`, real and already fetched) and `types/contract.ts` in
full before deciding what each of the three named report types should map to. **Conclusion: none of the
three needed a new data domain, and no fourth ("module") data source was invented for the one apparent
gap.**

| Named report | Resolves to | Why |
|---|---|---|
| **Bug Report** | `Finding[]` (`ReportsPage.tsx`'s own `findings` state) | Already real, already fetched, already the exact rows rendered on this page. No new fetch. |
| **Test Report** | `DashboardResponse` + `KpisResponse` (`aiqaApi.dashboard.get`/`.kpis`, BE-12/BE-12b) | Real, project-scoped execution/task counts and pass/fail totals — the same real endpoint Dashboard/FE-26 already renders live. `ReportsPage.tsx` does not otherwise keep this in state, so it is fetched fresh at export time. |
| **Final Audit Report** | A **computed reconciliation** of the two above (`buildFinalAuditRows`) | Not a fourth data source. Directly matches ACC-016's own "reconcile ... compare totals/statuses with authoritative run data" wording: total findings vs. total failed tasks vs. total runs/tasks, from data already in the export. |

**Final Audit Report is deliberately *not* the real Audit Trail** (`AuditPage.tsx`'s `AuditRow` actor-action
log — who did what). That is a genuinely different real concept; reusing it here would have been a name
collision with ACC-016's own text, not an honest match to what it actually asks for.

**"module-specific report"** (the Verification section's own second bullet): no `module` entity exists
anywhere in this backend's schema — re-confirmed directly against `types/contract.ts` before writing this.
`Finding`, `FailureCluster`, `DashboardResponse`, and every other real record here is scoped only by
`project_id`. Rather than fabricate a module filter, the export states this plainly (see the "Report Scope"
row below) and reuses the one real scoping boundary this app already has everywhere else — the selected
project. "Full project report" and "module-specific report" therefore honestly collapse to the same real
thing.

**Deliberately deferred, not included in this export:** per-finding `FailureCluster` detail (signature,
root-cause hypothesis, occurrence count, reproducibility). It is real, but the backend only exposes it via
a per-finding, on-demand `POST .../findings/:id/analysis` call (`aiqaApi.findings.analysis`, already used
by this page's own detail view) — there is no bulk list endpoint. Bulk-fetching it for every finding at
export time would mean one POST per finding, a materially larger and slower change than "export the data
already fetched for this page." Left for a later unit if wanted; not silently reinterpreted as already
covered.

---

## 2. What was built

**Libraries selected, and why.** No PDF or spreadsheet library existed in `forentend` before this unit.

- **PDF:** `jspdf` (v4.2.1) + `jspdf-autotable` (v5.0.8) — pure client-side, no backend change. Every
  section of this report is structured tabular data, which is exactly what `autoTable` is built for; a
  heavier, layout-oriented library (`pdfmake`, `@react-pdf/renderer`) was not justified for three tables
  and a title block.
- **Excel:** `xlsx` (SheetJS Community Edition, v0.18.5) — the standard client-side library for generating
  real, openable `.xlsx` files with multiple sheets, which maps cleanly onto "three report types" as three
  sheets in one workbook.
- **`xlsx`'s one known advisory, accepted as-is:** `npm audit` flags `xlsx@0.18.5` **high** for
  [GHSA-4r6h-8v6p-xvw6](https://github.com/advisories/GHSA-4r6h-8v6p-xvw6) (prototype pollution) and
  [GHSA-5pgg-2g8v-p4x9](https://github.com/advisories/GHSA-5pgg-2g8v-p4x9) (ReDoS). Both require *parsing*
  an untrusted, attacker-supplied spreadsheet/file — this app's usage is write-only (`XLSX.utils.aoa_to_sheet`
  /`.book_append_sheet`/`writeFile` on our own already-fetched, already-typed in-memory data; nothing here
  ever calls `XLSX.read`/`.readFile` on user-supplied input), so neither advisory's attack surface is
  exercised. No fix is available on the public npm registry (`fixAvailable: false` — SheetJS publishes
  patched builds only via their own CDN, not npm); pulling a tarball from a non-registry source to chase an
  inapplicable advisory was judged a worse trade than accepting it with this stated reasoning, matching this
  project's established risk-acceptance pattern from the Security Remediation unit.

**Real generation, from real fetched data, at export time.** `handleExportPDF`/`handleExportExcel`
(`ReportsPage.tsx`) share one `fetchExportReportData` call (`Promise.all([dashboard.get, dashboard.kpis])`),
then build three sections/sheets from real data only:

1. **Bug Report** — `buildBugReportRows(findings)`: id, title, severity, confidence, review state, created
   at, promoted at/by. Directly reuses `ReportsPage.tsx`'s own already-fetched `findings` state — no new
   fetch, no invented field.
2. **Test Report** — `buildTestReportRows(dashboard, kpis)`: total tests, passed/failed, every real
   `execution_counts`/`task_counts` entry, execution reliability, average task/run duration (the last two
   via this file's own existing `formatPct`/`formatMs` null-safe helpers — a genuinely `null` KPI renders
   `—`, never a fabricated `0`/`0ms`).
3. **Final Audit Report** — `buildFinalAuditRows(findings, dashboard, projectLabel)`: total findings, total
   runs/tasks/failed-tasks, a "Report Scope" row stating the project-level scope explicitly, and a
   reconciliation sentence ("N findings recorded against M failed tasks — a Finding is only auto-drafted
   once a failure signature recurs across >= 2 task executions, so fewer findings than failed tasks is
   expected, not an inconsistency").

All three row-builders are pure functions (no `jspdf`/`xlsx` calls), exported and directly unit-tested —
same convention as `TelemetryPage.tsx::parsePrometheusText` — so the PDF and Excel exports are provably
built from the exact same numbers and can never silently disagree with each other.

**A real, empirically-confirmed font-encoding fix.** jsPDF's built-in standard fonts (Helvetica — no custom
font is embedded) only render WinAnsi-encoded text. Verified directly, not assumed: a literal "≥" in a
table cell made `autoTable` fall back to per-character glyph positioning for that whole cell (visibly
garbled), and a real em dash silently vanished. `toPdfSafeRows`/`toPdfSafeText` normalize en/em dashes,
"≥", curly quotes, and ellipses to ASCII equivalents **only at the jsPDF boundary** — the shared row-builders
above keep their normal punctuation (this file's existing "—" null-placeholder convention included) for the
Excel path and for direct unit testing, since neither has this constraint.

**Dead-code found and removed while building this.** Both `handleExportPDF`/`handleExportExcel` initially
carried a `testRows.length > 0 ? testRows : [placeholder]` fallback, copied from the (legitimate) equivalent
pattern for Bug Report. On inspection this was unreachable: `dashboard` is always the real, non-null
`DashboardResponse` `fetchExportReportData` just resolved (a failed fetch throws and is caught before this
line runs), and `buildTestReportRows` always emits at least 3 rows once `dashboard` is non-null — so the
`[]` branch, and therefore the placeholder, can never execute here. Removed rather than left as misleading
defensive code implying a scenario that cannot occur; `buildTestReportRows`'s own unit tests (below) cover
the true null-dashboard case directly.

**Dynamic import, added after measuring a real bundle-size regression.** `jspdf`/`jspdf-autotable`/`xlsx`
(plus jsPDF's own transitive `html2canvas`/DOMPurify code, pulled in regardless of which jsPDF features this
file uses) added ~250 modules when imported statically. A real `vite build` comparison:

| | Before dynamic import | After dynamic import |
|---|---|---|
| Main bundle | 1,785.22 kB (539.15 kB gzip) | 1,071.13 kB (302.82 kB gzip) |
| Report-export code | *(inline in main bundle)* | 3 separate chunks, ~851 kB (~282 kB gzip), fetched only on first Export PDF/Export Excel click |

`handleExportPDF`/`handleExportExcel` now `await import('jspdf')`/`import('jspdf-autotable')`/`import('xlsx')`
instead of a top-of-file static import — every other page's initial load is unaffected by this unit's new
dependencies.

| File | Change |
|---|---|
| `forentend/src/components/pages/ReportsPage.tsx` | Module doc comment updated with the Bug/Test/Final-Audit-Report resolution above. Added `buildBugReportRows`/`buildTestReportRows`/`buildFinalAuditRows` (+ header const exports) and `toPdfSafeText`/`toPdfSafeRows`. Added `exporting`/`exportError` state, `fetchExportReportData`, `handleExportPDF`, `handleExportExcel`. Added "Export PDF"/"Export Excel" buttons (`FileDown`/`FileSpreadsheet` icons) next to the existing "Export JSON" button, gated on a selected project (not on `findings.length`, unlike Export JSON — a project with zero findings but real execution history still has a real report to generate) |
| `forentend/src/components/pages/ReportsPage.test.tsx` | **11 new tests** (6 direct unit tests of the row-builders; 5 component tests). Added two `vi.mock` calls (`jspdf`, `xlsx`) at module top level — both libraries assign their real download-triggering method (`save`/`writeFile`) in a way plain `vi.spyOn` cannot intercept (`save` is a per-instance own property, not on the prototype; `writeFile` is a non-configurable export) — confirmed empirically before choosing this approach, not assumed. Added `extractPdfText`, a small test-only helper that reconstructs a real jsPDF document's visible text from its raw content stream, needed because `autoTable` legitimately word-wraps long cell content across multiple draw calls (real, correct rendering) which breaks a naive `doc.output().includes(sentence)` check |
| `forentend/package.json` / `package-lock.json` | Added `jspdf@^4.2.1`, `jspdf-autotable@^5.0.8`, `xlsx@^0.18.5` |

Sidebar/Command Palette/routing needed no change — this unit extends the existing `/reports` page in place,
it does not add a new route.

---

## 3. Tests added — proving real data, not blob existence

Per this unit's own instruction, every export test asserts on **actual generated content**, not just that a
file/blob was produced:

- **Unit (6):** `buildBugReportRows` maps every real `Finding` field for both an unreviewed and a reviewed
  finding (including the real `promoted_at`/`promoted_by`), and produces zero rows (not a placeholder) for
  a real empty list. `buildTestReportRows` maps real `DashboardResponse`/`KpisResponse` fields including a
  genuinely `null` KPI rendering `—` (never a fabricated `0`), and produces zero rows for a `null` dashboard
  (the "fetch not yet resolved" case). `buildFinalAuditRows` reconciles real totals and states the
  project-level scope, and reconciles to real zeros (not fabricated placeholders) for `null` inputs.
- **Component (5):**
  - Export PDF/Export Excel stay disabled until a project is selected.
  - Export PDF: the real generated PDF's reconstructed text (`extractPdfText`) contains an actual finding's
    real title, the real `test_count` from the mocked backend, and the real reconciliation sentence.
  - Export Excel: the real generated workbook has exactly the sheets `['Bug Report', 'Test Report', 'Final
    Audit Report']`; `XLSX.utils.sheet_to_csv` on each sheet contains real finding data, real counts, and
    the real "Report Scope" row.
  - **Empty-project case**, run for both formats: zero findings + a real (successful, non-null)
    `DashboardResponse` with genuinely zero counts. This surfaced a real design question mid-test-writing —
    see §5 below — and the final assertions check for the honest "No findings recorded for this project."
    message (Bug Report) and the real zero-value rows `Total Tests 0`/`Passed 0`/`Failed 0` (Test Report),
    plus the real `0 findings recorded against 0 failed tasks` reconciliation. `DEF-881` (a known
    previous-version fake string) is asserted absent.
  - A real `500` on the dashboard fetch at export time shows a real error message and **never** calls
    `save`/`writeFile` — no partial or fabricated report is produced on failure.

---

## 4. Verification

```
cd forentend && npx vitest run    → 370 passed, 0 failed, 20 files (was 359/20 before this unit)
cd forentend && npx tsc --noEmit  → clean
cd forentend && npx vite build    → clean, 2967 modules transformed (was 2716 at this document's last
                                     baseline / 2718 as of the immediately prior unit); main bundle
                                     1,071.13 kB / 302.82 kB gzip (down from 1,785.22 kB / 539.15 kB gzip
                                     before the dynamic-import change — see §2)
```

Net new tests: **+11 (359→370)** — 6 direct unit tests of the row-builders, 5 component tests, all in
`ReportsPage.test.tsx`.

Backend was not touched this unit (no `.rs` file edited) — the last-verified **190 passed, 0 failed, 1
ignored** (`cargo test`, cited in the immediately prior unit's report) stands; not re-run here since nothing
backend-attributable changed.

---

## 5. Acceptance Matrix updated

`AIQA_ACCEPTANCE_MATRIX_v1.0.md`:

- **§5 grid:** ACC-016 Final moved `IMPLEMENTED` → `TESTED`.
- **§5A:** ACC-016's row fully rewritten with the investigation conclusion, what was built, what was
  tested, and why Final stays `TESTED` and not `VERIFIED` — the Bug/Test/Final-Audit-Report mapping is this
  unit's own reasoned conclusion from ACC-016's text, not something independently reconciled with the
  product owner, and the deferred FailureCluster-detail gap is real, not cosmetic.
- **§34:** `Frontend Build` and `Frontend Automated Tests` corrected from their stale 22-August-rebuild
  values (2716 modules; 339 passed/17 files) to the current real numbers (2967 modules; 370 passed/20
  files) — per this unit's own explicit instruction not to skip this. `Final Audit Report` moved
  `NOT_GENERATED` → `GENERATED (real, tested)`, with an explicit note that this document's own manual
  reconciliation work is a separate real audit, not the generated-report feature this field tracks.
- **§35:** Frontend Owner evidence line corrected to `370 passed / 20 files`.
- **Tally line:** moved from `9 VERIFIED, 8 IMPLEMENTED/TESTED` to `8 VERIFIED, 9 IMPLEMENTED/TESTED` to
  reflect ACC-016's move. While re-deriving this, the *pre-edit* tally was independently recomputed
  directly from the §5 grid's own Final-column values and found to already be a miscount before this unit's
  change (the real pre-edit split was 8 VERIFIED / 9 IMPLEMENTED-TESTED, not the stated 9/8) — corrected
  alongside the ACC-016 update, with the discrepancy disclosed in the tally line's own footnote rather than
  silently fixed.

---

## 6. What remains unresolved / what this report deliberately did not do

- **The Bug/Test/Final-Audit-Report mapping in §1 is this unit's own reasoned conclusion, not a product
  decision.** It is directly grounded in ACC-016's own Acceptance Criteria and Verification text and in the
  real data model, but unlike the six decisions in `AIQA_PRODUCT_DECISIONS_RECORD.md`, no one-question-per-item
  confirmation round was run for it (the task's own instruction was to conclude and state the reasoning, not
  to seek a decision) — flagged, not hidden, and why Final is `TESTED` rather than `VERIFIED`.
- **Per-finding `FailureCluster` detail is not in the export** — real, but bulk-fetching it would require N
  POSTs at export time (one per finding); deferred, not silently reinterpreted as covered. See §1.
- **Did not fabricate any field.** Every PDF/Excel cell traces to a real `Finding`, `DashboardResponse`, or
  `KpisResponse` field, or to plain arithmetic over them (totals, the reconciliation sentence). No new
  backend endpoint, no new data source.
- **Did not add a fake "module" filter.** Stated plainly, in the export itself, that no such entity exists.
- **Did not mark ACC-016 `VERIFIED`.** Real, tested report generation now exists for all three named report
  types in both formats, but the mapping above is this unit's own conclusion and the FailureCluster gap is
  real — §5A says so precisely.
