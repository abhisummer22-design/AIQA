# AIQA — Implementation Plan (Ready-to-Run Prompts)

**How to use this document:** run the prompts below through Claude Code **one at a time, in this order**.
Each item has a short "what we're building/checking and why" explanation in plain language first — read
that before running the prompt, so you always know what the next unit is actually for. Each prompt is a
**natural-language instruction, not code** — Claude Code writes the code; this document only tells it what
to do and why, matching how every unit in this project has been built so far.

Do not skip ahead to a P2 (Build) item whose P1 (Decision) hasn't happened yet — the prompt will either
refuse or guess, and neither is what you want. Do not run two prompts in the same session back-to-back
without reading that unit's report first — every prior unit in this project has been reviewed before the
next one started, and that discipline is what has kept this document trustworthy.

---

## Step 1 — Confirm ACC-022's new VERIFIED status is real

**What and why:** a recent session upgraded ACC-022 (Recovery/Fault Tolerance) to the highest possible
status, `VERIFIED`, based on its own real crash-and-restart test. But it verified its own work — nobody
independent has checked it yet, and this project's own history shows that self-checked claims have missed
real defects before. This step gets a fresh pair of eyes on that one specific claim before anything else
proceeds. Run in a **fresh Claude Code session with no memory of the upgrade.**

```
Independently re-verify ACC-022's recent upgrade from IMPLEMENTED to VERIFIED in
AIQA_ACCEPTANCE_MATRIX_v1.0.md, using the discrimination-by-reversion technique already established in
this codebase: temporarily revert or disable the recovery mechanism the claim depends on (e.g.
recover_incomplete_runs in engine.rs) in the live source, confirm the original symptom genuinely returns
under the same real orchestrator-process-kill-and-restart test, then restore it. Also independently
re-check the specific admission made while upgrading this status — that "unrelated tasks continue while
one worker fails" is described as "undemonstrable" in this single-process architecture — and confirm
whether VERIFIED is still the right call given that admission, or whether it should be held at TESTED with
the undemonstrable dimension named explicitly, applying the same standard already used for ACC-005 and
ACC-013. Report exactly what was checked, the technique used, and any defect or disagreement found.
```

---

## Step 2 — Confirm E2E-003's downgrade to PARTIAL was the right call

**What and why:** the same session also downgraded E2E-003 (API Flow) from PASSED to PARTIAL because
p95/p99 latency data genuinely can't be produced anywhere in this system. A downgrade is lower-risk than an
upgrade, but it was still a self-check — worth one more look in the same pass as Step 1, not its own
separate session.

```
As part of the same independent review as ACC-022, also re-check E2E-003's downgrade from PASSED to
PARTIAL in AIQA_ACCEPTANCE_MATRIX_v1.0.md §28. Confirm directly (not by re-reading the prior report) that
no latency/response-time/p95/p99 computation exists anywhere in this system's real code, and confirm the
PARTIAL status and its stated reasoning are accurate and consistent with how ACC-013 was already handled
for the identical gap.
```

---

## Step 3 — Broader independent verification sweep

**What and why:** Steps 1 and 2 checked the two newest, highest-risk self-verified claims. This step
widens the same check to a representative sample of the rest of the self-verified backend and frontend
work, since the same "verified by the same session that built it" pattern applies everywhere in this
project.

```
Using a fresh session with no memory of writing the original units, and using the same
discrimination-by-reversion technique already established in this codebase (temporarily reverting a
claimed fix in the live source and confirming the original symptom actually returns, then restoring it),
independently re-verify a representative sample of the self-verified work — at minimum the realtime event
stream (BE-14), the evidence/findings surface (BE-15), and a handful of the frontend units covering Test
Intelligence, the Execution Hub, and the AI router/Skills panel. Report exactly what was checked, what
technique was used for each check, and any defect found, in the same evidence-based style as the two Phase
1 verification reports.
```

---

## Step 4 — Human click-through of all 15 pages *(you do this one, not Claude Code)*

**What and why:** no automated test substitutes for a real person clicking through the whole product in a
real browser. This is exactly the manual testing pass already planned — do it once Steps 1–3 come back
clean, so nothing you're clicking through is standing on an unconfirmed claim.

Mint a fresh API key, start `start.bat` (now correctly pointed at the real frontend on port 3000), and walk
all 15 pages in order: create a project, launch a real execution against at least two different executors,
watch it complete live, review its evidence, confirm a finding gets drafted from a failure, review it in
Test Intelligence, and confirm every honestly-stubbed page shows its disclosed "not implemented"/"deferred"
state rather than an error. Note anything that looks wrong, confusing, or inconsistent — bring it back to
this conversation for triage.

---

## Step 5 — Resolve the Ollama / AI-runtime dispute

**What and why:** one earlier session found and used a real Ollama install; you separately said your
system has none. The AI Model Gateway's live-verified "GO" status rests on that disputed observation. This
step doesn't touch code — it's a decision-gathering step, so run it, get the report back, and answer its
question here before the next prompt runs anything live.

```
AIQA_AI_MODEL_AUTO_DISCOVERY_IMPLEMENTATION_REPORT.md discloses an unresolved contradiction: this session
found, started, and successfully used a real local Ollama installation and model — and then the user
stated their system has no Ollama installed at all. Before doing anything else with the AI routing
feature: report back clearly what AI model runtime (if any) is actually available on this machine right
now — do not assume either prior claim. State the real, current, agreed-upon fact of what AI model runtime
is meant to be available in the real target deployment, so the product owner can confirm it.
```

Once you have that answer, run this follow-up:

```
Now that the real available AI model runtime is confirmed, run the existing #[ignore]-marked live
differential test (different skill selections producing genuinely different generated test content
against a real model) for real against that confirmed runtime, and update the AI Model Gateway's verdict
in AIQA_ACCEPTANCE_MATRIX_v1.0.md based on what actually happens — do not mark it GO on the strength of
the earlier, now-resolved dispute alone.
```

---

## Step 6 — Decide the fate of the 7 honest stub pages and ~12 missing backend domains

**What and why:** AIQA Orchestrator, Custom Testing, Mobile Testing, Feature Map/Flow, and RBAC Testing's
scenario-matrix view (Test Case Validator and API Testing are already resolved) are real, honest gaps —
each needs a real decision, not an assumption, on whether to build it or formally cut it from scope. This
prompt only produces recommendations and asks the question one item at a time; it does not build anything
yet.

```
Seven frontend pages (AIQA Orchestrator visualizer, Custom Testing, Mobile Testing, Feature Map/Flow, RBAC
Testing's scenario-matrix view — Test Case Validator and API Testing are already resolved via redirect and
excluded here) and roughly a dozen contract-specified backend domains (context ingestion, quality-graph
queries, requirement traceability, RBAC scenario-matrix evaluation, generic API-test dispatch/telemetry,
mobile/device-farm dispatch, custom test suites, report generation as a real durable entity, the full
Model Gateway beyond today's single-task router) currently have zero backend implementation and are
honestly disclosed as such. For each one, produce a short, one-page recommendation covering: what it would
take to build for real, what it would take to formally descope it instead, and which of those this
project's actual trajectory suggests. Present these one at a time for a real build-or-descope decision on
each, the same pattern already used successfully for the six prior product decisions in
AIQA_PRODUCT_DECISIONS_RECORD.md. Once decided: for anything descoped, reissue the PRD/FRD/SRD at a new
version number whose stated scope actually matches what was just agreed — do not leave the governing
specification describing a product larger than what's being built with no version marking the difference.
For anything to be built, propose a lowest-effort/risk-first, highest-value-with-real-dependents-next
order, and implement only the first item in its own single, clearly labeled, independently reportable
unit — leave the rest queued for their own subsequent prompts.
```

*(After this runs and the decisions are recorded, come back here — the queued "build" items become new,
individually-scoped prompts at that point, the same way today's four-item build sequence was sequenced
after its own decision round.)*

---

## Step 7 — Decide the fate of the six remaining fake Settings tabs

**What and why:** General, SSO, Execution, Environments, Policies, and Notifications still show fake
controls under a standing "don't touch" instruction. That was fine short-term; it's a trust problem
permanently. This is its own small decision round, separate from Step 6's larger one.

```
SettingsPage.tsx carries a standing instruction, written into its own header comment, to preserve
pre-existing fake content in the General, SSO, Execution, Environments, Policies, and Notifications
tabs — only the Authentication tab and the AI Intelligence panel are real. Get an explicit decision from
the product owner on each remaining fake tab: build it for real on a stated timeline, or replace its fake
controls with the same honest "not yet available" pattern already used successfully on the stubbed pages
elsewhere in this product. Implement whichever is decided, tab by tab, preserving the Authentication and
AI Intelligence panels exactly as they are.
```

---

## Step 8 — Make selected Skills actually affect generated output

**What and why:** the Skills panel lets you pick different skills, but by its own disclosure, picking a
different one currently produces identical output. This closes that gap for real. No decision needed
first — this is scoped, independent work.

```
The Skills & MCP Phase 1 panel lets a user genuinely select and deselect skills, and that selection is
captured into the generation session — but, by the feature's own explicit disclosure, selecting a
different skill does not currently change the generated test-case content at all. Design and implement a
real connection from selectedSkillIds through to the AI-routed generation call in ai_generation.rs, such
that a genuinely different skill selection produces a genuinely different, observably different prompt or
output — and prove this with a test that selects two different real skill combinations and asserts the
actual generated content differs, not merely that a parameter was passed through.
```

---

## Step 9 — Build a real MCP connector backend

**What and why:** "Connect MCP" currently and correctly says no MCP backend exists. This is the larger,
independent unit that makes that dialog real — connecting to an actual MCP server and surfacing its tools.

```
The "Connect MCP" dialog currently and correctly tells the truth: no MCP backend, connector, or credential
store exists anywhere in this project. Design and implement Phase 2 of the Skills & MCP work: a real MCP
client integration that can connect to at least one real MCP server, discover its tools, and surface them
as real, selectable SkillDefinition entries of type MCP. Handle any required credentials the same way this
project already handles AI-provider credentials — never logged, never serialized into a client-visible
response. Do not mark "Connect MCP" as functional until a real connection to a real MCP server has
actually been demonstrated and tested, not merely coded.
```

---

## Step 10 — Real reachability checks for hosted AI providers

**What and why:** Gemini, OpenAI, and Anthropic currently only report "configured" from an environment
variable — never an actual live check. Once you have a real API key for at least one, this closes that
gap. Skip this step until you actually have a credential to test against.

```
Gemini, OpenAI, and Anthropic currently only report configured: true when their respective API key
environment variable is present — reachable is hardcoded false for all three. Now that real credentials
are available for at least one of these providers, add a genuine, bounded, low-cost reachability check
(analogous to Ollama's real /api/tags call) for each, and make sure a failed or expired credential is
reported honestly rather than silently staying in whatever state it last had. Extend the existing
credential-non-exposure tests to cover these new calls.
```

---

## Step 11 — Housekeeping: fix the stale §34 frontend test count

**What and why:** a small, already-known drift — §34 still says 370, the real number is 371. No
dependency on anything else; safe to run whenever convenient.

```
AIQA_ACCEPTANCE_MATRIX_v1.0.md §34's "Frontend Automated Tests" field still states 370 passed; run the
real frontend suite and confirm/correct it to the real current count, citing the fresh run.
```

---

## Not a Claude Code prompt: `apps/web`'s fate

This one is a plain decision, not engineering work — decide whether to delete, archive, or keep the
abandoned `apps/web` directory, and say so here when ready. No prompt needed until that's decided.

---

*This document contains no source code. Every prompt above is a natural-language instruction for Claude
Code to execute and report back on — the same pattern used for every unit built in this project so far.
See `AIQA_DEVELOPMENT.md` for the reasoning and priority behind each step.*
