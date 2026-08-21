# AIQA — Phase 4 — FE-16 Test Generator Truthfulness & Fake Execution Handoff Removal Implementation Report

## 1. Status

**PASS.**

## 2. Objective

Remove the confirmed runtime fabrication in the Test Generator (hardcoded commercial model names presented as selectable, false "Synthesized via {model}" claims, a fabricated `ExecutionRun` created on "export to execution") without building the real Model Gateway that would actually justify those claims. The page must become truthful about being a local/template-based generator, and its former fake execution handoff must no longer exist as a functioning product flow.

## 3. Decision Implemented

**FE-15 GAP-02 — Option A: Truthful Simulation Disclosure.**

Concretely, this means: the Test Generator keeps its exact current local/template generation mechanism (nothing about *how* it generates test cases changed), but every place that mechanism was described, labeled, or metadata-tagged as if a real AI model had performed it was corrected to say what actually happens — local template assembly, no model call. Separately, and independently of that disclosure, the fabricated execution-run creation on export was removed entirely, since FE-15 established no real `GeneratedTestCase → TaskDef` mapping exists to make a real handoff possible — Option B (a real Model Gateway) was explicitly not implemented.

## 4. Initial Evidence Baseline

Confirmed fabrication present before this unit (re-verified by direct inspection at the start of this unit, not assumed from the FE-15 report):
- **Provider branding**: header banner read `"GEMINI 2.0 FLASH REASONING"`.
- **Model selector**: an "AI Reasoning Engine" `<select>` offering `gemini-2.0-flash`/`gemini-1.5-pro`/`claude-3-5-sonnet` as if they routed generation to a real provider; selecting a value had zero effect on the generated output.
- **Local simulation presented as AI**: `handleTriggerGenerate`'s 3 chained `setTimeout` calls (1000+1400+1800ms), with a `PROCESSING` stage literally labeled "Analyzing Project Sources...".
- **False synthesis metadata**: every generated case's `traceability.aiAnalysis` was set to `` `Synthesized via ${model} (${depth} depth)` ``; `TraceabilityGraph.tsx`'s own fallback for a missing value was `'Synthesized via Gemini Quality Engine'`.
- **Fake execution export**: `handleExportToExecution` called `AIQAContext.triggerNewRun`, which fabricated a complete local `ExecutionRun` (random `run-####` id, hardcoded `totalTests: 10`/fake pass-running-queued counts, two fake named tests unrelated to what was generated), stored it in a state (`AIQAContext.runs`) nothing else in the app ever read, then navigated to the real Execution Hub — which never consumed it.

## 5. Files Inspected

`forentend/src/components/pages/TestCaseGeneratorPage.tsx` (full file), `forentend/src/context/AIQAContext.tsx` (full file), `forentend/src/types.ts` (full file), `forentend/src/types/contract.ts`, `forentend/src/components/test-generator/GenerationConfigSection.tsx` (full file), `forentend/src/components/test-generator/GeneratedTestCasesView.tsx` (full file), `forentend/src/components/test-generator/TraceabilityGraph.tsx` (full file), `forentend/src/components/pages/AIQAOrchestratorPage.tsx` (the one other file referencing `triggerNewRun`, confirmed to be a historical doc comment only), `forentend/src/components/pages/SettingsPage.tsx` (confirmed its own separate, already-disclosed-local-only model dropdowns are out of scope for this unit).

## 6. Changes Made

| File | Change |
|---|---|
| `forentend/src/types.ts` | Removed `TestExecutionItem` and `ExecutionRun` interfaces (dead once `triggerNewRun` was removed). Removed `model` field from `GenerationSession`. |
| `forentend/src/context/AIQAContext.tsx` | Removed `runs`/`triggerNewRun` from `AIQAContextType`, the state, the seed data (2 fabricated runs, 9 fabricated named tests), and the function itself. Removed `model` from the seeded `activeGenerationSession`. Removed now-unused `ExecutionRun`/`TestExecutionItem`/`AIQAStatus` imports. |
| `forentend/src/components/pages/TestCaseGeneratorPage.tsx` | Removed `model`/`setModel` state. Removed `triggerNewRun`/`currentEnvironment` from `useAIQA()` destructure. Rewrote `aiAnalysis` to a truthful string. Renamed `handleExportToExecution` → `handleOpenExecutionHub`, body reduced to `navigate('/execution')` only. Changed header banner text, the `PROCESSING` stage label, the `addActivity` completion title/description, and the `'ai-generated'` tag to `'template-generated'`. |
| `forentend/src/components/test-generator/GenerationConfigSection.tsx` | Removed the `model`/`onModelChange` prop and the "AI Reasoning Engine" `<select>` block entirely. Reworded the subtitle and the "AI Analysis" pipeline-visualization node to "Local Assembly". |
| `forentend/src/components/test-generator/GeneratedTestCasesView.tsx` | Renamed `onExportToExecution` prop → `onOpenExecutionHub`. Reworded the toolbar subtitle. Replaced the "Dispatch N to Execution" button with "Open Execution Hub" (always enabled, no longer gated on `approvedCount`), added a `title` attribute and a visible one-line honest disclosure caption beneath it. |
| `forentend/src/components/test-generator/TraceabilityGraph.tsx` | Step 3: label "AI Reasoning" → "Generation Method", badge "GENAI" → "LOCAL", status "SYNTHESIZED" → "TEMPLATE", fallback value corrected. Top summary badge "Synthesized" → "Prepared". |
| `forentend/src/components/pages/TestCaseGeneratorPage.test.tsx` (new) | 9 new tests covering truthfulness, fake-handoff removal, and regression (see §16). |

## 7. Provider / Model Claim Cleanup

- **Removed branding**: `"GEMINI 2.0 FLASH REASONING"` → `"LOCAL TEMPLATE GENERATION"` (page header badge).
- **Removed/reframed selector**: the "AI Reasoning Engine" provider dropdown was removed entirely, not disclaimed-in-place — it had zero real behavioral effect (confirmed: the `model` value was only ever echoed into the now-corrected metadata string), so per the brief's own §7 "If no real behavioral difference exists, remove the fake selector/control rather than inventing new behavior" it was deleted. The genuinely local, already-inert-but-honest "Exploration Depth" control was kept — it never claimed provider/AI involvement, only a local generation-intensity preference, and removing it was not evidenced as necessary.
- **Final truthful terminology**: "LOCAL TEMPLATE GENERATION" (header), "Local Assembly" (pipeline node, replacing "AI Analysis"), "Generation Method" / "TEMPLATE" / "LOCAL" (traceability step 3, replacing "AI Reasoning" / "SYNTHESIZED" / "GENAI"). No new fake model/provider/gateway name was invented anywhere, per the brief's explicit prohibition.

## 8. Generation Behavior

- **What local generation actually does, unchanged by this unit**: `handleTriggerGenerate` still assembles a fixed per-category template (title/objective/steps/assertions hardcoded per `switch (cat)`), interpolating cosmetic values (document labels, target URL) into strings — it does not analyze uploaded document content. This mechanism itself was explicitly not in scope to change (FE-15 Option A keeps the local/template behavior; only its labeling had to become truthful).
- **Generation states that remain**: `VALIDATING` ("Validating Sources...") — kept as-is; it doesn't make an AI claim, and real client-side validation (categories/docs/notes) does occur immediately before this stage. `PROCESSING` — relabeled "Preparing Test Templates..." (was "Analyzing Project Sources...", which falsely implied real document analysis). `GENERATING` ("Generating Test Cases...") — kept as-is; it is truthful, since cases genuinely are being assembled at that point.
- **Why each is truthful now**: none of the three remaining labels claims AI reasoning, model inference, or provider processing — they describe real local workflow stages (validate inputs → prepare templates → assemble cases), matching what the code actually executes.

## 9. Generated Metadata

- **Old value**: `` `Synthesized via ${model} (${depth} depth)` `` — a specific false claim naming a real commercial model.
- **Final truthful behavior** (Option B of the brief's §9 — retained, not removed, since `traceability`/`aiAnalysis` is used elsewhere legitimately by `TraceabilityGraph.tsx`): `'Locally assembled from a fixed category template — no AI model was called.'` The `TraceabilityGraph.tsx` fallback (used only when no `traceability` object exists at all, e.g. the pre-existing demo seed data) was corrected the same way: `'Locally assembled from template (no AI model called)'`. The shared `TraceabilityChain` type itself was not changed — only the values written into its `aiAnalysis` field.

## 10. Fake Execution Handoff Removal

- **Old `handleExportToExecution` flow**: filter approved cases → call `AIQAContext.triggerNewRun({title, environment, executor, concurrency})` → fabricate a complete `ExecutionRun` (random id, hardcoded counts, 2 fake named tests) → `setRuns((prev) => [newRun, ...prev])` → `navigate('/execution')`.
- **Old `triggerNewRun` fabrication**: the entire function existed solely to manufacture that fake object; it had exactly one real caller in the whole codebase (confirmed by a repo-wide search before removal).
- **Final behavior**: `handleOpenExecutionHub` is now `() => navigate('/execution')` — nothing else. The button that triggers it was renamed "Open Execution Hub", is always enabled, carries a `title` attribute stating cases aren't automatically sent, and the panel below it shows a permanent one-line disclosure: *"Generated cases stay local to this page — opening the Execution Hub does not send them to the backend or create a run."*
- **Confirmation no fake run is created**: live-verified (§17) — clicking the button navigates to the real, backend-driven Execution Hub with no new run, no fake ID, no fake counts, and no fabricated test names anywhere in its output.

## 11. `AIQAContext` Cleanup

- **`triggerNewRun` status**: removed. Before removal, a repo-wide search (`grep -R "triggerNewRun" forentend/src`) found exactly one real call site (`TestCaseGeneratorPage.tsx`'s former `handleExportToExecution`) and one unrelated historical doc comment (`AIQAOrchestratorPage.tsx`, describing a fabrication FE-11 already removed from that page — left untouched, not a live reference).
- **Remaining consumers**: none. After removing the one real call site, `triggerNewRun` had zero consumers left, satisfying the brief's own removal condition (§13: "remove the method only if unused").
- **Removed fields/types**: `AIQAContext.runs` (state) and its 2-entry, 9-fake-test seed array; the `ExecutionRun`/`TestExecutionItem` interfaces in `types.ts` (their only consumers were `AIQAContext.tsx`'s now-removed `runs`/`triggerNewRun`, confirmed via search before deletion). `GenerationSession.model` and `SystemSettings.aiModels` (Settings-page-only) were handled separately — see below.
- **Why anything remains**: `AIQAContext` itself was **not** deleted or gutted — it still provides real, actively-used state: `activeGenerationSession`/`setActiveGenerationSession` (the Test Generator's own legitimate session-persistence mechanism, unrelated to the fabrication), `addActivity` (a real write-only activity log the Test Generator still uses honestly), `settings`/`updateSettings` (the Settings page's own, separately-disclosed local-only configuration, out of scope for this unit), `currentEnvironment`, sidebar/command-palette UI state. `SystemSettings.aiModels.defaultModel: 'gemini-2.0-flash'` was deliberately **not** touched — it belongs to the Settings page's own pre-existing, already-disclosed-as-local-only pattern (established since FE-3), a different page this unit's brief explicitly did not authorize touching ("global UI redesign" is out of scope).

## 12. Approved UI Preservation

Confirmed preserved: page layout, the two-branch (Source & Generation / Generated Test Cases) navigation structure, the 5-document-slot upload UI, the 4-application-source connection UI, the 14-category selection grid, the generated-case list/filter/detail workspace, the approve/reject review footer, and the 8-step traceability chain's visual structure (same 8 steps, same card layout — only step 3's text content changed). No component was redesigned; no layout, spacing, or visual hierarchy was altered. The only UI removed was the fake model selector (justified by §7's own "remove rather than invent" rule) and the only UI added was one disclosure caption and a `title` attribute.

## 13. Backend Boundary

**Confirmed: no backend changes.** `git diff --stat -- services/orchestrator/` produced no output, both before and after this unit's changes. `services/orchestrator/` was not opened for editing at any point.

## 14. API Boundary

**Confirmed: no fake or new API was invented.** No `POST /generate`, `/model`, `/ai/generate`, `/tests/generated`, or `/execution/from-generated-tests` endpoint was added to the frontend transport layer or called. The Test Generator still makes zero `aiqaApi` calls, exactly as before this unit — its truthfulness fix is entirely about labeling and metadata, not about adding a real (or fake) backend integration.

## 15. Fabrication Search Verification

```
grep -RniE "gemini|claude-3|openai|gpt-|ollama" forentend/src
grep -RniE "synthesized|AI reasoning|model inference|live AI" forentend/src
grep -Rn "triggerNewRun|ExecutionRun" forentend/src
```

Remaining matches, all classified:
- `types.ts`, `AIQAContext.tsx`, `GenerationConfigSection.tsx`, `TestCaseGeneratorPage.tsx`, `TraceabilityGraph.tsx` — explanatory `// FE-16:` comments documenting what was removed and why. **Legitimate documentation, not runtime fabrication.**
- `AIQAContext.tsx`'s `SystemSettings.aiModels.defaultModel: 'gemini-2.0-flash'` and `SettingsPage.tsx`'s own model dropdown options/labels — the Settings page's separate, pre-existing, already-disclosed-local-only pattern (FE-3). **Out of scope for this unit**, not touched.
- `AIQAOrchestratorPage.tsx:9` — a historical doc comment describing FE-11's own past removal of a different fabrication from that page. **Legitimate documentation, no live reference.**
- No remaining match anywhere claims a real provider/model was called by the Test Generator at runtime. No remaining match is a live `triggerNewRun`/`ExecutionRun` reference.

**Confirmed**: a runtime page (the Test Generator) no longer claims a provider/model was called.

## 16. Tests

| | Count |
|---|---|
| Before FE-16 | 272 passing (14 files) |
| After FE-16 | **281 passing (15 files)** |
| New tests added | 9 (`TestCaseGeneratorPage.test.tsx`, new file) |

New tests cover: no hardcoded provider name rendered as the active engine; no fake model selector (while confirming the genuine "Exploration Depth" control remains); generation completes locally with no "Synthesized via..." text and truthful metadata present; no "AI Analysis"/"AI Reasoning" text anywhere on either branch; opening the Execution Hub creates no fake run/ID/counts/test names and truthfully navigates; the honest disclosure caption is shown (and no false export-success text); `AIQAContext` no longer exposes `runs`/`triggerNewRun`; the page still renders and local approve/reject review still works; local category generation still produces real, category-tagged cases.

Exact command/result:
```
npx vitest run
Test Files  15 passed (15)
     Tests  281 passed (281)
```

## 17. Manual Verification

Real backend (`aiqa-orchestrator.exe`) and real frontend both started; driven live in-browser with a freshly minted real Admin API key.

- **Scenario A**: opened Test Generator — header badge reads "LOCAL TEMPLATE GENERATION" (not Gemini), master UI (branches, tabs, layout) fully intact.
- **Scenario B/C**: switched to "Source & Generation" branch — confirmed no "AI Reasoning Engine" dropdown exists in the real rendered page; "Exploration Depth" control genuinely present; subtitle reads "...local template-based generation"; pipeline visualization reads "Sources → Local Assembly → Test Generation".
- Clicked "Generate Test Cases" with real category selections and requirement notes — generation completed for real (9 new category-tagged cases appeared, header switched to "Test Cases Generated").
- **Scenario D**: inspected a newly generated case's (`TC-FUN-001`) traceability chain — step #03 reads exactly `TEMPLATE / Generation Method / "Locally assembled from a fixed category template — no AI model was called."`; zero mention of any provider/model name anywhere on the page.
- **Scenario E**: clicked "Open Execution Hub" (confirmed button text, no "Dispatch"; confirmed `title` attribute and the visible disclosure caption both present) — navigated to the real Execution Hub, which showed its own real, honest "No project selected" state (this session had not selected a project) — no fake run, no fake ID, no fake counts, no fabricated test names, no false success message.
- **Scenario F**: reloaded the app at `/execution` — no fabricated execution state persisted (session auth reset to its normal unverified state, as expected; nothing fake survived because nothing fake was ever created).
- **Scenario G**: confirmed live that the real Execution Hub reads only real backend state.
- **Scenario H**: confirmed via `git diff --stat -- services/orchestrator/` (empty, both before and after) — no backend files changed.

Both backend and frontend processes were stopped after verification.

## 18. Typecheck / Build

```
npx tsc --noEmit
```
Exit 0, no output — clean (confirms no dangling references to the removed `ExecutionRun`/`TestExecutionItem`/`model` anywhere in the project).

```
npx vite build
```
```
✓ 2711 modules transformed.
dist/assets/index-BzrQDukA.js  1,017.57 kB │ gzip: 290.12 kB
✓ built in 39.93s
```
Succeeded; bundle size decreased slightly (fake run/test seed data and the removed selector's code were larger than the small amount of new disclosure text added).

## 19. Regression

- **Dashboard**: unaffected — not touched, its own tests still pass.
- **Execution Hub**: unaffected — not touched; live-verified (§17) it still reads only real backend data, and confirmed automated tests for it still pass.
- **Audit**: unaffected — not touched.
- **Evidence**: unaffected — not touched.
- **Realtime**: unaffected — not touched.
- **Existing FE-1 through FE-15 behavior**: confirmed via the full suite — 281/281 passing, including every pre-existing test file unchanged in behavior (the prior 272 all still pass verbatim; only the new 9 are additions).

## 20. Security

Confirmed: no fake credentials or provider API keys were introduced or exposed anywhere in this unit's changes. No real or fake provider key handling was added — the Test Generator still makes zero external/backend calls related to generation. The one real API key involved in manual verification (§17) was a freshly minted, disposable local test key, never hardcoded into source.

## 21. Remaining Runtime Fabrication

Explicitly listed, per the brief's required distinction:

**Truthful local/template behavior that remains (not fabrication, correctly disclosed):**
- The fixed per-category template content itself (title/objective/steps/assertions) — unchanged, and now correctly labeled as local/template-based rather than AI-generated.
- The 3-stage `VALIDATING → PROCESSING → GENERATING` local pipeline with its `setTimeout` delays — kept as real local workflow staging, with truthful labels.
- The "Exploration Depth" control — a real local preference that is stored and echoed into the generated case's `testData` JSON, honestly presented (it doesn't claim AI/provider involvement).
- `activeGenerationSession`'s pre-seeded demo data (3 demo cases, unrelated to any user action) — pre-existing, out of this unit's specifically-evidenced scope; its `aiAnalysis` now correctly falls back to the truthful string via `TraceabilityGraph.tsx`'s corrected fallback.

**False AI/provider claims that do NOT remain:** the "GEMINI 2.0 FLASH REASONING" banner, the "AI Reasoning Engine" model selector, "Synthesized via {model}", "AI Analysis"/"AI Reasoning" labels on the traceability chain, the `'ai-generated'` tag.

**Fake execution creation that does NOT remain:** `triggerNewRun`, the fabricated `ExecutionRun`, fake run IDs, fake pass/running/queued counts, fabricated test names, and any `AIQAContext.runs` mutation from this page.

## 22. Limitations

- **A real AI Model Gateway is not implemented in FE-16** — this is explicit, intentional, and matches FE-15 GAP-02 Option A exactly. The Test Generator remains a local/template generator; it does not analyze uploaded document content, does not call any real model, and does not have any real backend generation capability.
- Two other pre-existing, already-disclosed fake-model surfaces (`SettingsPage.tsx`'s own model dropdowns, `AIQAContext.defaultSettings.aiModels`) were deliberately left untouched — they belong to a different page's own separate, pre-existing FE-3 disclosure pattern, and touching them was out of this unit's scope.
- The demo-seeded `activeGenerationSession` (3 pre-existing generated cases shown on first load, unrelated to any user-triggered generation) was not removed — its existence was not itself evidenced as GAP-02 fabrication by FE-15 (it never claims a provider was called, and its `aiAnalysis` is undefined, correctly falling back to the truthful string), and removing it was not requested in this unit's scope.

## 23. Out of Scope

Confirmed **not** implemented in this unit:
- AI Model Gateway.
- Real provider integration (Gemini, Claude, OpenAI, Ollama, or any other).
- Backend generated-test persistence.
- `GeneratedTestCase → TaskDef` mapping.
- Real Test Generator → Execution Hub handoff (the button now honestly navigates only; it does not transfer any data).
- Any backend modification.
- FE-12 status-vocabulary migration.
- Global UI redesign (only the Test Generator's specific fabricated surfaces were touched; Settings page, Dashboard, Execution Hub, and every other page were left untouched).

## 24. Final Exit Criterion

| # | Criterion | Status |
|---|---|---|
| 1 | No longer falsely claims Gemini/Claude/another provider performed generation | **PASS** |
| 2 | Fake provider/model selection removed or truthfully reframed | **PASS** (removed) |
| 3 | `"Synthesized via {model}"` removed/converted to truthful metadata | **PASS** |
| 4 | Local/template generation not presented as real model inference | **PASS** |
| 5 | Fake `ExecutionRun` creation removed from the Test Generator flow | **PASS** |
| 6 | No fake run IDs or execution metrics generated | **PASS** |
| 7 | No false execution/export success shown | **PASS** |
| 8 | No real Model Gateway falsely claimed or partially invented | **PASS** |
| 9 | No backend API invented | **PASS** |
| 10 | No backend code/database modified | **PASS** |
| 11 | Test Generator approved UI remains visually intact | **PASS** |
| 12 | Existing FE-1 through FE-15 functionality does not regress | **PASS** (281/281) |
| 13 | Typecheck passes | **PASS** |
| 14 | Tests pass | **PASS** |

**All 14 criteria satisfied. FE-16: PASS.**
