# AIQA — Detailed Built-in Skill Instructions (Test Case Generator)

**Status: built and verified on branch `claude/richer-builtin-skills` (off `master` `6273384`).
Not pushed, no PR — waiting for the user's go-ahead.** The full backend suite (1,603 tests) and
the new/updated tests for this change (147) pass; a full-suite `cargo clippy --all-targets` is
clean of new warnings. An investigation into a test that failed during verification (§7) found it
to be a pre-existing, load-dependent flake, unrelated to this change — see §7 for the evidence.

## 1. Why this was built

The user opened a built-in skill (e.g. "Edge Case Analysis") in the Skills & MCP panel's "View
this skill's real instruction" dialog and found only 4-5 lines of text, and judged the skill
itself as not accurate/proper. Inspection confirmed this: every one of the 14 built-in skills
(`skillRegistry.ts`'s `BUILT_IN_SKILLS`) resolved, via the backend's
`ai_generation.rs::builtin_instruction`, to a single one-sentence instruction (25-70 words) —
what technique to apply, boundary rules, checklists, and examples were absent, unlike the user's
own dropped `SKILL.md` files in `services/orchestrator/skills/` (e.g. `02-test-case-design.SKILL.md`,
4.6 KB, with worked examples for Equivalence Partitioning, Boundary Value Analysis, decision
tables, and state transitions). This is the real text embedded in the AI generation prompt
(`build_prompt`/`build_requirement_prompt`), so a thin instruction here means thinner, more
generic generated test cases.

The user's instruction: **"jo important skill hai usko rakho and update kro taaki test case ki
accuracy increase ho"** ("keep the important skills, and update them so test case accuracy goes
up") — i.e. widen the 14 built-in skills' own instructions, do not remove anything (no built-in
skill, no custom-skill path, no `SKILL.md`-file path was removed), per the project's standing
"additive, minimal changes" rule.

## 2. Module name and location

**No new file/module was created.** The addition lives inside the existing `ai_generation`
module — `services/orchestrator/src/ai_generation.rs`, in a new section named (in the source)
`// ------------------------------------------- detailed built-in instructions`, immediately
before the existing Skill Resolver. Two call sites in `services/orchestrator/src/requirement_generation.rs`
and one route in `services/orchestrator/src/api.rs` were updated to use it. This matches the
existing module's own convention (`ai_generation.rs` already holds the Skill Resolver, the Wave
Partitioner, the Prompt Builder, and the Feedback Context builder as sections of one module,
not separate files).

**New public surface, all in `ai_generation.rs` unless noted:**

| Name | Kind | Purpose |
|---|---|---|
| `DETAILED_SKILLS_MIN_WINDOW_TOKENS` | `pub const u32 = 7_000` | The prompt-window floor below which a built-in skill sends its old short instruction instead of the new detailed one |
| `CASE_STANDARD` | `const &str` | The shared "what every case must look like" text (one condition per case, concrete steps, observable result, title/priority/severity rules) appended to every detailed instruction |
| `builtin_detailed_focus(name)` | `fn -> Option<&'static str>` | The new, skill-specific detailed text (techniques + checklist + boundary) for each of the 14 built-in skills; `None` for anything else |
| `builtin_detailed_instruction(name)` | `fn -> Option<String>` | `builtin_detailed_focus` + `CASE_STANDARD`, joined |
| `resolve_skill_instructions_for_window(skills, window_tokens)` | `pub fn -> Vec<ResolvedSkillInstruction>` | Resolves each skill to its detailed form, unless `window_tokens` is known and below the floor above (then the old short form) |
| `resolve_skill_instructions_for(infra, skills)` | `pub async fn -> Vec<ResolvedSkillInstruction>` | Same, using the real provider window the router reports for `AiTask::TestCaseGeneration` right now |
| `resolve_skill_instructions(skills)` | `pub fn` (existing name, kept) | Unchanged signature; now always the short form — used only where a caller has no window to check (mainly the resolver's own unit tests) |

## 3. How it works

**The problem a longer instruction creates:** Ollama's default context window is 4,096 tokens
(`README.md`'s own `OLLAMA_NUM_CTX` note), and a requirement-driven generation call already
spends most of a small window on the fixed prompt text, the batch's requirement text, and the
earlier cases it is told not to repeat (`RESERVED_TOKENS = 3_500` in `requirement_generation.rs`).
A skill instruction several times longer would not leave room for that, and Ollama silently cuts
a too-long prompt from its start.

**The fix:** every one of the 14 built-in skills gets two forms:
- **Short** (unchanged): the original one-sentence instruction every earlier release sent.
- **Detailed** (new): a focused paragraph naming the concrete techniques and checklist for that
  skill (e.g. Edge Case Analysis: minimum-1/minimum/minimum+1 boundary testing, empty/blank
  values, Unicode text, out-of-order sequences, empty/full collections), plus the shared
  `CASE_STANDARD` block (one condition per case, concrete step values, an observable expected
  result, title/priority/severity rules) that every skill's cases must follow.

A call picks the detailed form **only when the provider that would actually serve it right now
reports (or is assumed to have) a prompt window of 7,000 tokens or more** — Groq's free tier
(8,000), Ollama with `OLLAMA_NUM_CTX=8192` set, or Gemini/another cloud provider. An **unknown**
window (the router could not determine one) is treated as **not small**, so a misconfigured or
newly-added provider defaults to the fuller instruction rather than silently degrading. Both real
generation paths (`ai_generation.rs::generate_one_wave` for pasted-text generation, and
`requirement_generation.rs`'s per-batch and per-top-up-round calls for requirement-driven
generation) now look up that real window once per call/round and pass it through.

**What is unaffected:**
- A **custom skill** (typed into the Skill Manager) or a **`SKILL.md` file** dropped into
  `services/orchestrator/skills/` still resolves to its own stated text, verbatim, on every
  window — this feature only changes the 14 built-in skills' own resolution.
- The **evidence rules**, the **JSON output schema**, and the **prompt structure**
  (`build_prompt`/`build_requirement_prompt`) are unchanged — the detailed text is additional
  content inside the existing "SELECTED SKILLS" block, not a new prompt section.
- `GET /api/v1/skills/resolve` (the "View this skill's real instruction" dialog the user opened)
  now shows the detailed form when a working provider is configured, so what the user reads
  there is provably the same text a real generation call would embed — same guarantee the
  route's own doc comment already made, extended to cover which form is shown.

## 4. Files changed

| File | What changed |
|---|---|
| `services/orchestrator/src/ai_generation.rs` | New section: `DETAILED_SKILLS_MIN_WINDOW_TOKENS`, `CASE_STANDARD`, `builtin_detailed_focus` (14 skills), `builtin_detailed_instruction`, `resolve_skill_instructions_for_window`, `resolve_skill_instructions_for`; `generate_one_wave` now calls the window-aware resolver; 10 new unit tests |
| `services/orchestrator/src/requirement_generation.rs` | `generate_from_requirements` and `run_top_up` each look up the real prompt window once and pass it to `resolve_skill_instructions_for_window`; import updated; 1 new test (`a_call_carries_the_detailed_skill_on_a_large_window_and_the_short_one_on_a_small_one`) |
| `services/orchestrator/src/api.rs` | `resolve_skills` (the `POST /api/v1/skills/resolve` handler) now takes `State<AppState>` and calls `resolve_skill_instructions_for` with the real router, instead of the always-short `resolve_skill_instructions` |
| `services/orchestrator/src/api_tests.rs` | 1 new API test: `the_skill_viewer_shows_the_detailed_built_in_instruction_and_a_custom_skill_verbatim` |
| `README.md` | `OLLAMA_NUM_CTX` section: noted the 7,000-token floor and what determines which form is sent |

No frontend file changed — `SkillDetailModal.tsx` already reads the instruction live from this
same route and displays whatever it returns, so it needed no change to show the longer text.

## 5. Example — Edge Case Analysis, before and after

**Before (short, still sent on a small window):**
> Identify: boundary values, null/empty values, malformed values, maximum/minimum values,
> unexpected state transitions, unusual input combinations, and concurrency-sensitive edge
> conditions where the requirement text gives a real basis for them.

**After (detailed, sent when the window allows):**
> Test the limits of what the requirement describes:
> - each numeric, length, count, size or date limit it states: just below, at and just above it
>   (minimum-1, minimum, minimum+1, maximum-1, maximum, maximum+1), using only limits the text
>   gives;
> - empty, blank, whitespace-only and missing mandatory values; a value of a type the field
>   accepts and one of a type it does not;
> - unusual but valid content: the longest allowed text, special characters, mixed case,
>   non-Latin (Unicode) text, leading and trailing spaces;
> - unusual sequences: repeating an action, doing steps out of order, cancelling half way, going
>   back or refreshing, two users acting on the same item;
> - empty and full collections: zero, one and many items.
>
> Each case changes one thing at the edge and expects what the requirement says at that limit.
> Plain rejection of clearly invalid input belongs to Negative Testing.
>
> *(+ the shared Case standard block: one condition per case, concrete step values, observable
> result, title/priority/severity rules.)*

All 14 built-in skills (Functional, Edge Case, Security, Performance, API, Database, RBAC,
Business Case, UI/UX, Non-Functional, Negative, Positive, Spike, Load) were written out this way,
each keeping the same subject its short form already had and naming where it ends and a
neighboring skill's subject begins (e.g. Edge Case vs. Negative Testing; Spike vs. Load).

## 6. Verification so far

Targeted tests (`ai_generation`, `requirement_generation`, `skill_files`, and the two new/updated
API tests) — **147 passed, 0 failed, 1 ignored**:
- Every built-in skill's detailed instruction is far fuller than its short one and ends in the
  shared case standard; a name with no built-in mapping (a custom skill) gets neither.
- A detailed instruction's own key words match its short instruction's (e.g. Security still says
  "authorization"/"injection"; Spike vs. Load stay distinct text).
- Every skill whose subject is a quantity (Performance, Non-Functional, Spike, Load, Edge Case)
  still says not to invent a number.
- The window threshold itself: 4,096 (Ollama's default) and one token below the floor both get
  the short form; the floor itself, 8,000 (Groq), and an unknown window all get the detailed
  form.
- A custom skill's own text is untouched on every window.
- A Functional-only prompt with the detailed instruction still contains no Security-only wording
  (e.g. "injection") — the shared case standard was written to name nothing skill-specific.
- The requirement-driven path picks the form the call's real window allows, batch by batch.
- The `/api/v1/skills/resolve` route returns the detailed form for a built-in skill and a custom
  skill's own text verbatim (no AI router configured in the test, so the window is unknown → not
  small → detailed, matching production's own "unknown is not small" rule).

`cargo fmt --all` was run; unrelated files it reformatted were reverted (`git checkout --`) so
this change touches only the 5 files listed above. `cargo clippy --all-targets` is clean of any
new warning (the two pre-existing `unused_imports` warnings in `mobile.rs` are unrelated to this
change). The full backend suite, `cargo test --lib` (1,603 tests), passes with this change in
place — see §7 for the one test that needed investigation along the way.

## 7. Investigated — a be12 failure during verification, root-caused as a pre-existing load flake

`scripts/ci/local-checks.ps1 -Only backend` first reported **FAIL**, not FLAKY:
`api_tests::be12_dashboard_and_kpis_reflect_real_run_and_task_data` failed in the full run and
again in the script's own alone/single-threaded retry, and then **15/15** when rerun alone
directly against the built test binary — on its face not the load-dependent flakiness the
project's own notes describe for the be12/be13/be15/be18 family (a fixed 200ms sleep before
asserting an async ingestion happened; that family normally *passes* when rerun alone), so it
was treated as a possible real regression and investigated before anything was committed.

This test asserts on the dashboard/quality-graph endpoint after posting one mixed-outcome
execution plan — code this change does not touch (no file in §4 is on that path), which made a
genuine regression from this change unlikely on its face. Root cause was confirmed by comparison:

| Build | Machine state during the run | be12 alone, repeated |
|---|---|---|
| This change's 4 files reverted to clean `master` `6273384` (rebuild took **16m 17s** — itself a sign of a heavily loaded machine at that moment) | Busy (a same-session background rebuild had just finished; several other apps using CPU) | Not tested at that busy moment |
| This change, same busy period | Busy | **15 / 15 failed** |
| This change's files reverted to clean `master`, re-tested once the machine had quieted (rebuild 16m 17s finished, load dropped) | Quiet (22% CPU, no other cargo/rustc process) | **0 / 8 failed** |
| This change restored, rebuilt again on the now-quiet machine (rebuild took **1m 48s** this time) | Quiet | **0 / 15 failed** |

Same test, same code both times on each side of the comparison — only the machine's load at the
moment differed, and the pass/fail result tracked the load, not the code. This is the same
load-dependent flakiness the project's own notes already document for this test family, now with
one more data point: under **sustained heavy load** (not just an occasional spike) it can fail
every time it is rerun, alone, single-threaded — "passes when rerun alone" only holds on an
otherwise-quiet machine. A full run of the whole suite (`cargo test --lib`, 1,603 tests) on the
quiet machine, with this change in place, passed **1,603 / 1,603** (2 ignored, 0 failed).

**Conclusion: this change is not responsible for the be12 failure seen during verification.** No
code change was made to address it — this is a pre-existing test-timing issue already tracked
separately, not something to fix as part of this feature.

## 8. Not done / explicitly out of scope

- The 50 `SKILL.md` files the user dropped into `services/orchestrator/skills/` (Playwright,
  Cypress, k6, JMeter, etc.) are untouched — they are a separate, file-backed mechanism and
  already carry their own detailed content.
- No duplicate `SKILL.md (1).md` files were removed (8 pairs noted during the earlier review) —
  a cleanup action, not part of this change.
- The prompt's evidence rules, output schema, coverage/top-up technique definitions
  (`coverage.rs::Technique::definition`), and the crawl-context block are unchanged.
- No new environment variable was added; `OLLAMA_NUM_CTX` (existing) is what raises Ollama's
  window past the new floor.
