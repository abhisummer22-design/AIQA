# AIQA — ACC-015 Scope Investigation & Deferred-Page Copy Honesty Pass

**Date:** 22 August 2026
**Scope:** Two ordered parts, investigate-before-touching-copy in both. Part 1: resolve the open ACC-015/
Custom Testing question `AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` §1.2 flagged without answering. Part
2: make the 4 remaining deferred stub pages (AIQA Orchestrator, Mobile Testing, Feature Map/Flow, RBAC
scenario-matrix) state their real, recorded, deferred status specifically instead of generically.

---

## PART 1 — ACC-015 / Custom Testing `focus` investigation

### What was investigated

Read the real code, not the page's name or its own prior doc comment's characterization. Three questions,
answered directly against source:

1. **Does a "focus" scoping input actually exist?** Partially. `GenerationConfigSection.tsx` (lines
   286–302) has a field labeled "Requirement Focus & Generation Notes" — but its state variable is
   `requirementNotes`, and `TestCaseGeneratorPage.tsx`'s own internal comment calls it "Requirement Prompt
   / Notes." It is a general free-text notes field, not a distinct scope parameter. **No "module" or
   "feature" selector exists anywhere on this page** — ACC-015 requires four selectable dimensions
   (project, module, feature, focused objective); at most a project-wide (not Test-Case-Generator-specific)
   project switcher and this one notes field exist.

2. **Does that field provably constrain which test cases get generated?** No. The only generator this page
   actually calls is the local, non-AI, fixed-template one (`handleTriggerGenerate`). Traced every use of
   `requirementNotes` in it: it is copied verbatim into exactly one metadata field (`sourceRequirement`)
   identically across every produced case. It does not filter `selectedCategories` (the real determinant of
   which cases get produced), and does not change any case's real title/description/steps/priority/severity
   — those are fixed per-category templates, unconditional on the field's content.

3. **Is it even sent to the backend's real AI-generation path, where scoping could plausibly work (the way
   skill selection provably changes prompt/output — proven in `ai_generation.rs`'s own tests)?** No. Direct
   search confirmed **zero** calls to `POST .../test-cases/generate-ai` exist anywhere in this frontend.
   `TestCaseGeneratorPage.tsx`'s own doc comment states this plainly: *"every generation call in this page is
   entirely local/simulated ... there is no real `aiqaApi` call anywhere in this file."* The backend's
   `GenerateAiTestCasesBody` schema (`api.rs`) has no `focus`/`module`/`feature` field either way —
   `requirement_text` and `skills` only.

### Conclusion — stated plainly

**No.** The Test Case Generator's "Requirement Focus" field does not satisfy ACC-015's narrower reading, in
any of the three required senses: it doesn't scope by module/feature (no such controls exist), it doesn't
provably constrain the local generator's output, and it isn't even wired to the one real backend path where
provable scoping is architecturally possible. This is a genuine, direct, negative finding — not "unclear" or
"partially."

### What changed as a result

Per this unit's own instruction for a "no" outcome: **`CustomTestingPage.tsx` was not touched.** ACC-015 was
not built, upgraded, or reinterpreted. Two documents that explicitly left this question open were updated to
record the resolution (not the underlying code):

- **`AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` §1.2** — appended a dated "Follow-up resolution" paragraph
  stating the investigation and the negative answer, and that ACC-015 stays `NOT_STARTED`.
- **`AIQA_ACCEPTANCE_MATRIX_v1.0.md` §5A** — ACC-015's row rewritten from a thin, pre-investigation citation
  (`routeConfig.tsx`) to the real evidence trail above. **§5 grid: unchanged** (ACC-015 was already
  `NOT_STARTED` across every column before this unit and remains so — no status cell was edited).

No new scoping logic was written anywhere, per the explicit instruction that a "no" finding here must not
become new build work.

---

## PART 2 — Deferred-page copy: specific instead of generic

### What was investigated first

Read `AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` §2 in full for the real, current, named revisit condition
of each of the 4 pages (not paraphrased from memory), and read each page's current `SectionHeader`/
`EmptyState` copy and `EmptyState.tsx`'s own prop surface before writing anything, to reuse the existing
component and props rather than inventing new ones.

### What changed — copy only, confirmed by a clean `tsc`/full build/full test run

No backend/frontend behavior changed on any of these 4 pages — same `EmptyState`/`SectionHeader`
components, same props, same (still-true) "no backend entity/route exists today" fact retained in each
description. Only the `badge`/`title`/`description`/`codeHint` text changed, per page:

| Page | ACC ref | Old badge | New badge | Real, named revisit condition now stated |
|---|---|---|---|---|
| `AIQAOrchestratorPage.tsx` | ACC-006 | `NOT IMPLEMENTED` / `NO BACKEND CAPABILITY` | `DEFERRED — REAL FUTURE SCOPE` / `DEFERRED, NOT DESCOPED` | "revisit if product direction shifts toward autonomous routing; today's DAG-scheduler orchestrator ... is the committed model" |
| `MobileTestingPage.tsx` | ACC-014 | same as above | same as above | "a real device-farm budget decision ... matching the PRD's own existing 'post-MVP' precedent for adjacent mobile tooling (Appium/Strix)" |
| `FeatureMapFlowPage.tsx` | ACC-003 (visual half) | same as above | same as above | "practically gated on Context Ingestion (ACC-002, itself deferred) — nothing real to visualize without it" |
| `RBACTestingPage.tsx` | ACC-011 | same as above | same as above | "real RBAC enforcement remains fully committed and already shipped ... only this dedicated evaluation tool is deferred" |

Every description also cites `AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` §2 directly (both inline and in
`codeHint`) and states the capability is real, intended product scope, deliberately deferred per a direct
product-owner decision — not an unexplained gap. Each page's own module doc comment was updated the same
way (a dated "Copy updated 22 August 2026" note explaining what changed and why, preserving the original
fabrication-history paragraphs unchanged since they remain accurate).

**`CustomTestingPage.tsx` was deliberately not touched in this part** — Part 1's own conclusion is that its
current generic copy remains accurate (ACC-015 stays `NOT_STARTED`, undecided-not-deferred is a genuinely
different status than the other 4, which do have a recorded decision).

**One deliberate non-change, disclosed rather than silent:** `routeConfig.tsx`'s `designStatus:
'unsupported-not-implemented'` value for these 4 routes was **not** changed. That field is documentation-
only (confirmed by a repo-wide search — nothing else reads it) and its own doc comment defines the bucket by
whether a real backend route/entity exists today, which is still true for all 4. The task scoped this pass
as copy-only; introducing a third `designStatus` bucket (real-but-deferred, distinct from both existing
buckets) would be new taxonomy work, not a copy change, so it was left for a future session rather than
decided here.

### Tests — proving the new copy renders and the old generic string is gone

`CaseCPages.test.tsx` was restructured (previously one shared 5-page table with identical assertions across
all 5; the split is required because Custom Testing's copy is genuinely unchanged while the other 4's is
not):

- **`customTestingCase`** (1 entry) — keeps the *original* assertions verbatim (`NOT IMPLEMENTED`/
  `NO BACKEND CAPABILITY` present, banned fabricated-content strings absent) — proving Part 1's conclusion
  that nothing here changed.
- **`deferredCases`** (4 entries) — new assertions per page: the new `DEFERRED — REAL FUTURE SCOPE`/
  `DEFERRED, NOT DESCOPED` badges render; a page-specific substring of its own real revisit condition
  renders; its ACC ref and the source document name render; **the old `NOT IMPLEMENTED`/
  `NO BACKEND CAPABILITY` strings are explicitly asserted absent** (`queryByText(...) === null`), not just
  unchecked; the original banned fabricated-demo-content strings remain asserted absent (still relevant); no
  real backend request beyond the generic `/projects` list call is made (a copy-only change must not add
  one).
- **A cross-page distinctness test** — renders all 4 pages in sequence and confirms their 4 revisit-condition
  substrings are 4 genuinely different strings, not one sentence reused four times.

Two real bugs were caught and fixed while writing these tests (both in the test assertions, not the page
copy): two revisit-condition phrases were deliberately echoed in both a page's title and its full
description (intentional emphasis, not a defect) — `getByText`'s single-match requirement doesn't tolerate
that, so those checks were changed to `getAllByText(...).length > 0`; and the `AIQA_PRD_FRD_SRD_v6.1_
SCOPE_CORRECTIONS.md` citation is deliberately stated twice per page (description + `codeHint`) for clarity,
which needed the same fix.

---

## Verification

```
cd forentend && npx vitest run    → 371 passed, 0 failed, 20 files (was 370/20 before this unit — net +1:
                                     CaseCPages.test.tsx restructured from 22 to 23 tests)
cd forentend && npx tsc --noEmit  → clean
cd forentend && npx vite build    → clean, 2967 modules transformed (unchanged from the immediately prior
                                     unit — no new dependency, copy-only change)
```

Backend was not touched this unit (no `.rs` file, no migration edited) — the last-verified **206 passed, 0
failed, 2 ignored** (`cargo test`, cited in the immediately prior unit's report) stands; not re-run here
since nothing backend-attributable changed.

---

## Acceptance Matrix updated

`AIQA_ACCEPTANCE_MATRIX_v1.0.md`:

- **ACC-015** — §5 grid unchanged (`NOT_STARTED`, as it already was). §5A rewritten with the real
  investigation evidence and negative conclusion (see Part 1 above).
- **No other ACC row changed.** The 4 deferred-page copy updates (Part 2) do not correspond to any
  Acceptance Matrix status change — ACC-006/ACC-011/ACC-014/ACC-003 were already, correctly, `NOT_STARTED`
  before this unit (the underlying capabilities are still not implemented; only how honestly each page
  explains that changed) and remain so. Nothing in this report claims otherwise.

---

## Confirmation — all 4 Part 2 pages now state their real status specifically

`AIQAOrchestratorPage.tsx`, `MobileTestingPage.tsx`, `FeatureMapFlowPage.tsx`, and `RBACTestingPage.tsx`
each now state, in their own rendered copy: (1) the capability is real, intended future product scope: (2)
it was deliberately deferred, not rejected, per a direct, dated product-owner decision; (3) its own specific,
real, named revisit condition, quoted from `AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md` §2 — not a shared
generic sentence; and (4) still-true, that no corresponding backend entity/route exists today. The old
generic "NOT IMPLEMENTED"/"NO BACKEND CAPABILITY" phrasing is gone from all 4, confirmed by tests that
assert its absence, not merely that new text was added alongside it. `CustomTestingPage.tsx` is unchanged,
per Part 1's own conclusion.
