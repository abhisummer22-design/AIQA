# AIQA — Phase 1: Skills & MCP Panel Implementation

## 1. Executive Summary

A real, extensible Skills & MCP selection architecture was added beside the existing Test Case Generator input area. Eight built-in skills are genuinely selectable/deselectable (real React state, not a decorative checkbox); a Skill Manager modal lets a user search, toggle, and create a session-only custom skill with an honest "not saved anywhere" disclosure; a Connect MCP dialog truthfully states that no MCP backend exists yet and never shows a fake "Connected" status. No existing Test Case Generator behavior — local template generation, the FE-16 truthfulness fixes, or the AUD-005 real persistence/validation/handoff work already in this page — was altered or broken. 18 tests exist against this page (13 pre-existing, unmodified, still passing; 5 new), full suite is 334/334, typecheck and production build are clean.

## 2. Files Inspected Before Implementation

`TestCaseGeneratorPage.tsx`, `GeneratedTestCasesView.tsx`, `GenerationConfigSection.tsx`, `DocumentUploadSection.tsx`, `ApplicationSourceSection.tsx`, `types.ts`, `types/contract.ts`, `services/aiqaApi.ts`, `context/ProjectContext.tsx`, `context/AuthContext.tsx`, `routes/routeConfig.tsx`, `components/layout/CommandPalette.tsx` (the only existing modal-shaped UI in the repo), `components/layout/Header.tsx`, `components/pages/DashboardPage.tsx`, `components/pages/SettingsPage.tsx` (existing config/credential-adjacent patterns), `TestCaseGeneratorPage.test.tsx`, `vite.config.ts`/`vitest.config.ts` (existing), and a repository-wide `grep -ri "skill|mcp|plugin"` across `forentend/src` and `services/orchestrator/src` (zero matches before this unit — confirmed no existing plugin/skill/MCP/model-provider architecture anywhere).

**Findings that shaped the design:**
1. Main input area = the `SOURCES_AND_GENERATION` branch's stacked cards (Documents, App Sources, `GenerationConfigSection`).
2. No existing responsive side-panel pattern to reuse structurally, but `GeneratedTestCasesView.tsx` already uses a `grid-cols-1 lg:grid-cols-12` + `col-span-N` idiom for its own two-column layout — reused for this panel's grid.
3. No reusable `<Modal>`/`<Drawer>` component exists — `CommandPalette.tsx`'s own `fixed inset-0 z-50` overlay + `bg-[#0B1120] border border-slate-700/80` card is the only existing modal-shaped pattern, and was reused verbatim for both new modals.
4. Frontend state is local `useState` per page/component, with `AIQAContext.activeGenerationSession` carrying generation-config continuity across branch switches — the same pattern selected skills now use.
5. `selectedCategories`/`depth` already exist as real, local generator options — skills are a new, parallel concept, not a rename of these.
6. Files needing modification: `TestCaseGeneratorPage.tsx` (wiring + layout), `types.ts` (one new optional field on `GenerationSession`), `TestCaseGeneratorPage.test.tsx` (new tests).
7. Zero pre-existing Skills/MCP concepts anywhere in the repository (backend or frontend) — confirmed by direct grep, not assumed.

## 3. Files Changed

| File | What changed | Why |
|---|---|---|
| `forentend/src/types/skills.ts` (new) | `SkillDefinition`, `SkillSourceType`, `SkillAvailability`, `CustomSkillDraft`, `McpConnectionDraft` types | The standardized skill model (brief §7) — a new, standalone module since nothing like it existed |
| `forentend/src/services/skillRegistry.ts` (new) | `BUILT_IN_SKILLS` (8 skills), `DEFAULT_SELECTED_SKILL_IDS`, `MCP_BACKEND_CONFIGURED = false` | The Skill Registry — real data, not embedded in the page component |
| `forentend/src/hooks/useSkillSelection.ts` (new) | `useSkillSelection()` — selection state, toggle, add-custom-skill | Selection state as a standalone, reusable hook, not inline page state |
| `forentend/src/components/test-generator/SkillsAndMcpPanel.tsx` (new) | The "Skills & MCP" panel: selected-skills list, Add Skill / Connect MCP buttons | The visible panel, reusing `GenerationConfigSection`'s own card/checkbox visual language |
| `forentend/src/components/test-generator/SkillManagerModal.tsx` (new) | Search + select/deselect + create-custom-skill modal | Brief §5C's Skill Manager, reusing `CommandPalette`'s overlay pattern |
| `forentend/src/components/test-generator/McpConnectModal.tsx` (new) | Honest "not configured" MCP disclosure dialog | Brief §6 — no real MCP backend exists, so no connection is attempted |
| `forentend/src/types.ts` | Added `selectedSkillIds?: string[]` to `GenerationSession` | Lets skill selection survive a branch switch the same way `selectedCategories`/`depth` already do |
| `forentend/src/components/pages/TestCaseGeneratorPage.tsx` | Wired `useSkillSelection`; restructured the `SOURCES_AND_GENERATION` branch into a `lg:grid-cols-10` (`col-span-7`/`col-span-3`) layout; captured `selectedSkillIds` into `activeGenerationSession` on generate | Places the real panel beside the real input area; makes selection reachable by a future generation engine |
| `forentend/src/components/pages/TestCaseGeneratorPage.test.tsx` | Added 5 new tests (rendering, selection, generation-integration, Add Skill, Connect MCP) | Brief §11 |

No backend file, migration, scheduler, engine, or auth/RBAC code was touched — none was needed for this unit.

## 4. Skills Architecture

```
Skill Definitions (BUILT_IN_SKILLS, services/skillRegistry.ts — 8 real,
selectable capability labels; MCP_BACKEND_CONFIGURED = false stated once,
authoritatively, here)
        ↓
Skill Registry (skillRegistry.ts exports the definitions + defaults;
future PLUGIN/MCP-sourced SkillDefinition values would be added here,
not inline in the page)
        ↓
Selected Skill State (hooks/useSkillSelection.ts — real React state:
skills[], selectedSkillIds[], toggleSkill(), addCustomSkill();
reusable by any component, not trapped in TestCaseGeneratorPage)
        ↓
Test Case Generator (TestCaseGeneratorPage.tsx calls the hook once,
renders <SkillsAndMcpPanel/> beside the input area, and captures
selectedSkillIds onto activeGenerationSession — real, in-scope
configuration, available to whatever reads that session next)
        ↓
Future Generation Engine / Plugin / MCP Layer (not built in this unit —
a real generation engine can read activeGenerationSession.selectedSkillIds
directly; a real plugin registry or MCP connector would add
SkillDefinition values of type PLUGIN/MCP to the registry and set
MCP_BACKEND_CONFIGURED — no rewrite of this page, the hook, or the panel
is required)
```

`SkillDefinition.type` (`BUILT_IN | CUSTOM | PLUGIN | MCP`) is the single extension point: adding a real plugin or MCP producer means adding values of that type to a registry, not changing this type, the hook's contract, or `SkillsAndMcpPanel`'s rendering logic — it already renders any `SkillDefinition` generically.

## 5. MCP Status

**No MCP backend architecture currently exists; truthful setup/unavailable state implemented.**

Confirmed by a repository-wide grep across `services/orchestrator/src` and `forentend/src` before writing any code — no MCP server config, connector, credential store, plugin registry, or model/provider-configuration architecture exists anywhere in AIQA. `McpConnectModal.tsx` states this directly to the user, shows a real `Not Configured` badge (never `Connected`), and collects zero input — no name/URL/credential field exists in this phase, so nothing is ever at risk of being stored insecurely.

## 6. What Is Actually Functional Now

- Selecting/deselecting any of the 8 built-in skills is real `useState`-backed application state, immediately reflected in the UI (checkbox icon swap, `aria-checked`).
- Two skills (Functional Testing, Edge Case Analysis) are pre-selected by default, matching the brief's own layout mock.
- "Add Skill" opens a real, functional Skill Manager: search filters the real skill list; toggling selection there updates the same shared state the main panel reads; creating a custom skill adds a real `SkillDefinition` (type `CUSTOM`) to state and selects it, with an explicit, truthful "added for this session only — not saved anywhere" disclosure — never a fake "Skill saved successfully" message.
- "Connect MCP" opens a real dialog that truthfully reports `Not Configured` and explains why, with no input collected and nothing stored.
- The existing local generation pipeline (`handleTriggerGenerate`) is unmodified and still completes exactly as before with any skill selection active.
- Selected skill ids are captured into `activeGenerationSession.selectedSkillIds` on generate, exactly like `selectedCategories`/`depth` already are.
- The panel sits in a `lg:grid-cols-10` grid (`col-span-7` input / `col-span-3` panel) beside the existing input stack on desktop, and stacks to a single column below `lg` — the same responsive-grid idiom (`grid-cols-1 lg:grid-cols-N` + `col-span`) already proven elsewhere in this exact codebase (`GeneratedTestCasesView.tsx`'s own detail layout).

## 7. What Is Prepared for Future Implementation

- `useSkillSelection`'s return shape (`skills`, `selectedSkillIds`, `toggleSkill`, `addCustomSkill`) is the exact, standalone contract a future generation-engine unit can consume without touching this page's other internals.
- `SkillDefinition.type` already models `PLUGIN`/`MCP` sources; a future real plugin registry or MCP connector only needs to produce values of those types into the registry.
- `activeGenerationSession.selectedSkillIds` is real, already-threaded state a future backend-integrated generation call can read and send.
- `McpConnectModal`/`MCP_BACKEND_CONFIGURED` are the single place a future real MCP connector's UI/status logic would change.

## 8. What Is NOT Implemented

- Selected skills do **not** yet change the local template generation's per-category output — the pipeline remains fully deterministic per category, as it was before this unit. Claiming otherwise would be exactly the kind of unproven "Security Testing/MCP improved the result" statement this unit is required not to make.
- No custom-skill persistence exists (no backend endpoint, no `localStorage`) — a custom skill is genuinely gone on reload, and the UI says so.
- No real MCP connector, credential storage, or plugin registry exists — `Connect MCP` is purely informational.
- No project- or account-level persistence of skill selection — it is session/UI state only, exactly as the brief permits for Phase 1.
- No live visual (real-browser, authenticated) confirmation of the responsive grid was performed this session — see §13 below for why, and why the automated-test + code-level evidence was judged sufficient instead.

## 9. Tests Run

```
cd forentend && npx vitest run src/components/pages/TestCaseGeneratorPage.test.tsx
```
18/18 passed (13 pre-existing, unmodified; 5 new): panel + built-in skills + Add Skill/Connect MCP render; default selection state and toggle-updates-real-state; existing local generation still completes with a skill selected; Add Skill creates a real, session-only custom skill with a truthful disclosure (and asserts no "saved successfully" text exists); Connect MCP asserts a `Not Configured` status and asserts no "Connected" text exists.

```
npx vitest run
```
**334/334 passed**, 16 files (329 pre-existing + 5 new), 0 failures.

## 10. Build Results

| Check | Command | Result |
|---|---|---|
| Typecheck | `npx tsc --noEmit` | Clean, no output |
| Full test suite | `npx vitest run` | 334/334 passed |
| Production build | `npx vite build` | Clean, 2716 modules transformed, same pre-existing chunk-size advisory |

No backend file was modified, so no backend build/test was run for this unit (correctly out of scope — confirmed via `git status`, only frontend files changed).

## 11. Fake/Stub Audit

Fresh `grep` for `fake|mock|demo|sample|placeholder|Math\.random|TODO|FIXME|hardcoded success|fake connection|simulated connection` across every new/changed file. Every match reviewed:
- HTML `placeholder="..."` attributes in `SkillManagerModal.tsx` — legitimate form affordances, not fake data.
- A doc comment in `useSkillSelection.ts` explaining *why* `Math.random()` is deliberately not used (a monotonic counter is used instead for custom-skill ids).
- A doc comment in `skillRegistry.ts` referencing "the brief's own layout mock" — documentation, not data.
- All remaining matches are pre-existing, already-disclosed items in `TestCaseGeneratorPage.tsx`/`types.ts` untouched by this unit (the FE-16 local draft id, the `DEMO` app-source type, historical fake-removal doc comments).

**No new fake data, fake connection, fake persistence, or fake execution was introduced.**

## 12. Final Evidence-Based Review

- **UI**: panel is in the same grid row as the input stack via `lg:grid-cols-10`/`col-span-7`+`col-span-3`, stacking to `grid-cols-1` below `lg` — confirmed by reading the rendered JSX/class structure; the specific Tailwind idiom is already proven in this codebase elsewhere (§6). No live-browser screenshot was captured this session (see §13).
- **State**: `aria-checked`/toggle assertions in the new tests directly prove selection is real component state, not a static checkbox.
- **Architecture**: skills are not "trapped" in `TestCaseGeneratorPage.tsx` — they live in `types/skills.ts`, `services/skillRegistry.ts`, and `hooks/useSkillSelection.ts`, each independently importable.
- **Truthfulness**: `McpConnectModal` never renders "Connected"; `SkillManagerModal` never renders "saved successfully"; both are asserted by tests, not just claimed here.
- **Regression**: all 13 pre-existing `TestCaseGeneratorPage` tests and the other 316 pre-existing tests across the suite pass unmodified.

## 13. Limitation Honestly Disclosed

A live, authenticated browser walkthrough (real backend + real project selection + visually confirming the 70/30 layout and mobile stacking with a screenshot) was attempted but not completed: the backend's existing SQLite database already has API keys from prior work, so the bootstrap-admin-key flow (which only mints a key when zero exist) does not fire, and no raw key is recoverable from the hashed store. Minting a fresh key would have required backing up and temporarily replacing that existing database file, which was judged an unnecessary risk to other in-progress work for a UI-layout check this session's automated tests, TypeScript types, and a directly-comparable, already-proven Tailwind grid pattern elsewhere in this codebase already provide strong indirect evidence for. This is disclosed here rather than fabricating a manual verification that did not happen.

## 14. Final Verdict

**COMPLETE FOR PHASE 1.**

All FINAL ACCEPTANCE CRITERIA are met except the one explicitly optional/best-effort item (a live-browser visual pass), which is honestly disclosed above rather than claimed. Every functional, truthfulness, regression, and build/test criterion is met and evidenced: the panel exists beside the input via a responsive grid, built-in skills are genuinely selectable via real state, the existing generator flow is unchanged and passing, the architecture is extensible (registry + hook + typed model, not a page-embedded array), Add Skill and Connect MCP are both truthful with no fake connection/persistence anywhere, and the full frontend suite, typecheck, and production build are all clean.
