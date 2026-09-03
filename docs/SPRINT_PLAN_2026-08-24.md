# AIQA — Execution Plan (Sprint 0 → 3)

**Author:** engineering review, 24 Aug 2026
**Input:** `AIQA_FRESH_FULL_PROJECT_AUDIT_REPORT.md` (24 Aug 2026), re-prioritized
**Baseline commit:** `0ab75de`
**Sizing basis:** one developer, AI-assisted, ~6 focused hours/day. Estimates are in dev-days.

---

## 0. What this plan changes from the audit's own ordering

The audit's findings are accurate — I independently confirmed the load-bearing ones against the code (§6). Its *priority order*, however, has three problems:

### Delta 1 — AUD-015 must move from P2 into Sprint 1, alongside AUD-001

The audit calls AUD-001 "the easiest kind of gap to close" and treats AUD-015 (cloud providers are `env::var()` stubs) as a separate P2 item. Ship them separately and here is what a user experiences on the day AUD-001 lands:

```
Old:  click Generate → 3 test cases appear (fake, but labeled, and useful-looking)
New:  click Generate → { test_cases: [], generation_method: "fallback",
                         fallback_reason: "no healthy provider" }
```

That is a **UX regression** unless a provider actually answers. Ollama isn't reachable on the audit machine, and no cloud executor exists — so `AiInfrastructure::generate` cannot succeed for anyone who doesn't self-host a model.

The good news: this is cheap. `ai_router.rs` already has the right abstraction — `trait ProviderDiscovery` (`:200`) and `trait AiExecutor` (`:380`), with executors held in a `HashMap<&'static str, Arc<dyn AiExecutor>>` (`:552`) and registered one line at a time (`:578`). Adding a real provider is **an implementation, not a refactor**.

### Delta 2 — AUD-023 (no CI) is Sprint 0, not P1

577 passing tests that nothing enforces are not a safety net; they are a claim in a markdown file. Every ticket below is a change to a codebase whose only regression protection is a human remembering to run four commands. CI is ~half a day and it protects all 3 sprints that follow. It goes first.

### Delta 3 — the fake-behavior findings get **deleted**, not disclosed

The audit offers "real implementation *or* honest disclosure" for AUD-004/005/006. Disclosure costs the same as deletion and leaves the fake code in the tree, where it will be re-discovered by the next audit and re-litigated. Default action: delete the fake path, render an honest empty/unsupported state. This codebase already has a good `EmptyState` pattern for exactly that.

---

## Sprint 0 — Make the repo defensible (1 day)

Nothing here ships user value. All of it makes the next three sprints cheaper and is a prerequisite for trusting any result.

| # | Ticket | Scope | Est |
|---|---|---|---|
| S0-1 | **CI pipeline** (AUD-023) | New `.github/workflows/ci.yml`: on push/PR run `cargo test` (in `services/orchestrator`), `npm ci && npm run lint && npm test && npm run build` (in `forentend`). Two jobs, Rust cache + npm cache. No deploy step. | 0.5 |
| S0-2 | **Fix the dirty working tree** | `git status` currently shows ~95 modified `.md` files with no content change — a `.gitattributes` / line-ending churn. Normalize (`git add --renormalize .`), commit once, and confirm a clean tree. Right now nobody can tell a real diff from noise. | 0.25 |
| S0-3 | **Delete `apps/web`** (AUD-027 + AUD-019) | `git rm -r apps/`. It is a buildable duplicate frontend with a committed Firebase key. Rotate that key in the Firebase console as part of this ticket — deleting the file does not remove it from git history. | 0.1 |
| S0-4 | **Move the ~95 root `AIQA_*_REPORT.md` files into `docs/history/`** | Root-level report sprawl is the single loudest signal of report-driven development in this repo. History stays in git; the root becomes readable. Keep only `README.md` + this plan at root. | 0.15 |

**Definition of done:** CI green on a PR to `master`; `git status` clean; repo root has under 10 files; `apps/` gone; Firebase key rotated.

---

## Sprint 1 — Make the core feature actually work (5–6 days)

**Goal:** a user selects skills, types a requirement, clicks Generate, and gets test cases that a real model produced and that demonstrably change when the skills change.

This is the entire thesis of the product. Everything else in the backlog is secondary to it.

| # | Ticket | Scope | Est |
|---|---|---|---|
| S1-1 | **API client for `generate-ai`** (AUD-001a) | `forentend/src/services/aiqaApi.ts` — add to the existing `testCases: { ... }` block (`:460`): `generateAi(projectId, { requirement_text, skills })`. Add matching types to `types/contract.ts` mirroring the Rust `GenerateAiTestCasesResponse` (`api.rs:738`): `test_cases`, `generation_method: 'ai' \| 'fallback'`, `provider?`, `model?`, `fallback_reason?`. | 0.5 |
| S1-2 | **Skill transport** (AUD-002) | `useSkillSelection.ts` (`:80`) currently ends at UI state by its own admission. Add a `resolveSelectedSkills(): SkillInputBody[]` returning `{id, name, description}` — the backend's `SkillInputBody` (`api.rs:717`) takes a `description`, so **custom skills work over the wire on day one** with no backend change. | 0.5 |
| S1-3 | **Rewire the Generate button** (AUD-001b) | `TestCaseGeneratorPage.tsx::handleTriggerGenerate` (`:422`) — replace the three-deep `setTimeout` + `switch(cat)` template chain with a real awaited call. Map `GeneratedTestCaseDraft[]` into the page's existing `GeneratedTestCase` view model. Delete the per-category template strings (~250 lines go away). | 1.5 |
| S1-4 | **Truthful result rendering** | Replace the static `LOCAL TEMPLATE GENERATION` badge (`:793`) with one driven by the real response: `AI · {provider}/{model}` vs. a `GENERATION UNAVAILABLE` state showing `fallback_reason` verbatim. **No silent re-fallback to templates** — that would recreate the exact deception the audit flags. | 0.5 |
| S1-5 | **One real cloud provider** (AUD-015, pulled forward) | `ai_router.rs`: implement `GeminiDiscovery` (real `GET /v1beta/models` call, populating a real model list) + `GeminiExecutor: AiExecutor`. Register at `:578`. Reuse the existing `ExecutionError` variants so router fallback/error paths need no change. Pick Gemini or an OpenAI-compatible endpoint — the `OpenAiCompatible` enum variant (`:47`) already exists and is currently dead. | 2 |
| S1-6 | **Tests** | Backend: capability/routing tests already cover selection; add one asserting the new executor surfaces a real `ExecutionError` on a 4xx rather than a fabricated success. Frontend: assert that generating with Security-only vs. Functional-only skills sends **different** `skills[]` payloads, and that a `fallback` response renders the unavailable state (not test cases). | 1 |

**Acceptance criteria (all must hold):**

1. Same requirement text + different selected skills ⇒ demonstrably different generated output.
2. The rendered result names the real provider and model that produced it.
3. With every provider down, the UI shows an honest failure — zero test cases, `fallback_reason` visible — and never falls back to templates.
4. A custom skill created in `SkillManagerModal` measurably alters the prompt (it is transported as `description`).
5. `grep -r "LOCAL TEMPLATE" forentend/src` returns nothing.

> **Scope discipline:** do not add AI features on top of this in the same sprint. The audit's §42 is right about that — an unwired foundation is the wrong thing to build on.

---

## Sprint 2 — Remove everything that lies to the user (4–5 days)

Sprint 1 makes the product true at its centre. Sprint 2 makes it true at its edges. These are trust bugs: each one is small, and each one, when a real operator finds it, discredits the parts that are genuinely excellent.

| # | Ticket | Action | Est |
|---|---|---|---|
| S2-1 | AUD-006 — pre-seeded fake "AI-generated" cases | **Delete.** `AIQAContext.tsx` `activeGenerationSession` defaults to `null`. Page renders its empty state. This is the worst of the four because the seeded cases carry no disclosure at all. | 0.25 |
| S2-2 | AUD-005 — fabricated "Connected" status | **Delete the fake, add the real one.** `ApplicationSourceSection.tsx::handleConnect` — either a real reachability check through a new backend endpoint (the SSRF guard in `executor.rs` already exists and must be reused — do **not** let the browser probe arbitrary URLs, and do not add a new unguarded server-side fetch), or remove the Connect button entirely. Never `Math.random()` a ping time. | 1 |
| S2-3 | AUD-004 — fake upload progress | **Make real.** `FileReader` → send text content as `requirement_text` to S1-1's endpoint. This is the natural input for the generator and closes the "traceability is fabricated" half of AUD-004 for free, because the backend already re-verifies `source_reference` as a literal substring. | 1 |
| S2-4 | AUD-007 — Settings tabs fake-persist | **Split the button.** The AI Models tab already has a real, separate save action. Every other tab: remove the shared "Save Settings" button and mark the tab `not-implemented` using the existing `EmptyState` pattern. Do not keep a green checkmark that means nothing. | 0.75 |
| S2-5 | AUD-008 / AUD-009 | **Delete.** Decorative "Rotate Token" card with no handler; environment switcher that affects zero API calls. Both are pure removals. | 0.25 |
| S2-6 | AUD-014 — `POST /runs` silently simulates | **Reject instead.** `ExecutorRegistry::resolve` (`executor.rs:49`) is currently infallible — it returns `Arc<dyn Executor>` and cannot express "unknown", so it hands back `SimulatedExecutor`. Change the signature to `Option<Arc<dyn Executor>>`, keep the `SimulatedExecutor` default only for an explicitly-absent `executor` field, and have the handler 400 with the valid executor list otherwise. Add a test. | 0.5 |
| S2-7 | AUD-016 — `xlsx` vulnerability | **Migrate to `exceljs`.** `forentend/package.json:28` pins `xlsx@^0.18.5` with two unfixed High advisories and no upstream fix. Only the Reports export path uses it. Contained, mechanical swap. | 1 |
| S2-8 | AUD-026 — stale README | Fix the "LLM integration not yet implemented" line, which Sprint 1 makes doubly wrong. | 0.1 |

**Definition of done:** a full click-through of the app finds no screen that reports success for an operation that did not occur.

---

## Sprint 3 — Close the scope questions (6–8 days, or fewer if you cut)

Sprints 0–2 are unambiguous — every ticket is "make the code match the claim." Sprint 3 requires **product decisions**, and each decision has a cheap answer and an expensive one. Decide before you estimate.

### Decision 1 — Performance / load / stress testing (AUD-012)

The audit is blunt and correct: no mechanic exists anywhere that generates concurrency, ramps traffic, runs for a duration, or asserts a threshold. Yet it appears as a named skill category in the UI.

- **Cheap (0.5 day):** remove the category from the taxonomy and mark it out of scope, exactly as Mobile/Custom/FeatureMap are already honestly marked. Score for that area goes from a hidden 0 to a disclosed "not in scope."
- **Expensive (4–5 days):** add a k6 executor. You already have a working pattern — five real subprocess adapters in `executor.rs` with `sanitize_token`, SSRF guard, and evidence capture. k6 emits JSON summaries that fit the existing evidence pipeline. This is the *only* item in the whole backlog that adds a genuinely new product capability rather than repairing an existing claim.

**Recommendation: expensive.** "Performance testing" as a category with nothing behind it is the largest remaining gap between what AIQA says it is and what it does — and the marginal cost is low because the executor pattern is already proven five times over.

### Decision 2 — Deployment target (AUD-024)

Right now the repo holds two contradictory architectures: a real single-process SQLite + Windows-only Job Object system, and an `infra/docker-compose.yml` describing Postgres/pgvector + Redis + MinIO that is not wired in, not running, and not referenced by any code. Keeping both is the most expensive option available.

- **Cheap (1 day):** commit to the single binary. Add a Dockerfile (Linux, drop the Job Object containment behind a `#[cfg(windows)]` with a documented Unix equivalent or an accepted gap), delete `infra/docker-compose.yml`, and write down "SQLite, single process, one node" as the deliberate architecture. It is a completely legitimate choice at this scale.
- **Expensive (5+ days):** actually migrate `store.rs` (1,868 lines of `sqlx`) to Postgres and wire the Python `services/workers` stub over Redis.

**Recommendation: cheap, now.** Migrate to Postgres when you have a concurrency problem, not before. The dual-architecture state is what is actually costing you today.

### Remaining Sprint 3 tickets

| # | Ticket | Action | Est |
|---|---|---|---|
| S3-1 | AUD-003 — custom skills session-only | Persist to a `custom_skills` table (migration `0008`), project-scoped. Sprint 1 already makes them functional over the wire; this makes them survive a reload. | 1.5 |
| S3-2 | AUD-010 — generated-case handoff always simulates | The honest fix is a script-generation step, which is a large feature. For now: keep the disclosure, and file it as an explicit roadmap item rather than a bug. **No code this sprint.** | 0 |
| S3-3 | AUD-013 — stub testing pages | Delete the routes rather than shipping five empty pages. A nav item that leads nowhere costs more trust than an absent one. | 0.5 |
| S3-4 | AUD-028 — execution activity timeline | Add a time-bucketed SQL aggregate endpoint; the frontend component and its empty state already exist. | 1 |
| S3-5 | AUD-025 — 1.07 MB bundle | Route-level `React.lazy` on the 14 routes in `src/routes/routeConfig.tsx`. | 0.5 |
| S3-6 | AUD-021 / AUD-017 | Scope `/metrics` to admin; bump `vitest` past the UI-server CVE (dev-only, but free). | 0.25 |

---

## Explicitly not doing (and why)

| Finding | Why deferred |
|---|---|
| AUD-011 — MCP | Not a bug; an unbuilt feature. Needs a product decision about what MCP is *for* in a QA control plane before any code. Currently disclosed honestly. |
| AUD-018 — prompt injection | Blast radius is the requester's own generation call. Real, but revisit when generation becomes multi-tenant or automated. |
| AUD-020 — docker-compose creds | Resolved for free by Decision 2's cheap path (file gets deleted). |
| AUD-022 — WS `?key=` | An accepted browser limitation with no better standard answer. Document it; don't churn on it. |
| Rust dependency CVEs | The audit flags these as **UNVERIFIED**, not absent — no `cargo audit` was run. Add `cargo audit` to the S0-1 CI job (~10 min) and let the result decide whether a ticket exists. |

---

## Risk register

| Risk | Mitigation |
|---|---|
| S1-5 slips (cloud provider API turns out fiddlier than expected) | S1-1→S1-4 still ship; the feature is real but only usable with local Ollama. Ship it that way rather than holding the sprint. |
| Deleting the template generator makes the demo look worse before it looks better | This is real and expected. A truthful "no provider configured" state is a better demo than fabricated output — and Sprint 1 is short. |
| The 371 frontend tests break when the generator is rewired | Expected: tests asserting local-template behavior *should* fail. Rewrite them against the new contract; do not weaken them to pass. CI (S0-1) is what makes this visible. |
| Scope creep into new AI features mid-Sprint 1 | The audit's §42 warning is the right one. One feature, wired end to end, before anything new. |

---

## Where this lands the scorecard

The audit's overall 6.8 is dragged down almost entirely by one integration gap. Sprints 0–2 do not add capability — they connect and truthfully label what already exists:

| Area | Now | After Sprint 2 | Driver |
|---|---:|---:|---|
| Test Case Generation | 3 | 8 | S1-3, S1-5 |
| Skills | 4 | 8 | S1-2 |
| AI Infrastructure | 7 | 8 | S1-5 (2 real providers, not 1) |
| Settings | 4 | 7 | S2-4 |
| Frontend | 7 | 8 | S2-1…S2-5 |
| Security | 8 | 9 | S2-7, S0-3 |

The honest read: **the backend is a 9 and the product is a 3.** Sprint 1 is where that gap closes, and it is roughly one week of work.

---

## Appendix — what I verified myself

Everything below was confirmed against the code at `0ab75de`, not taken from the audit report:

- `POST /api/v1/projects/:project_id/test-cases/generate-ai` is registered at `api.rs:96`, handled at `:756`, Operator-gated and project-scoped. **Confirmed.**
- `GenerateAiTestCasesBody { requirement_text, skills: Vec<SkillInputBody> }` at `api.rs:717-728`; `SkillInputBody` carries `{id, name, description}`. **Confirmed** — custom skills need no backend change.
- `aiqaApi.ts` (504 lines) has `testCases` (`:460`) and `ai` (`:488`) blocks and **no** `generateAi` function. **Confirmed.**
- `TestCaseGeneratorPage.tsx::handleTriggerGenerate` at `:422` is a nested `setTimeout` chain over `selectedCategories` with a per-category `switch` at `:455`; `selectedSkillIds` appears only in local state and in a session record at `:695`. **Confirmed.**
- `ai_router.rs` provider plumbing: `trait ProviderDiscovery` `:200`, `ApiKeyProviderDiscovery` registered for gemini/openai/anthropic at `:347-349` (env-var check only), `trait AiExecutor` `:380`, `OllamaExecutor` `:384`, executor map `:552`, single registration at `:578`, "no executor wired for provider" failure at `:687`. **Confirmed — pluggable, no refactor needed for S1-5.**
- `forentend/package.json:28` pins `xlsx@^0.18.5`. **Confirmed.**
- `AIQAContext.tsx:112` initialises `activeGenerationSession` with a non-null seeded object. **Confirmed** — AUD-006 is real.
- `ExecutorRegistry::resolve` (`executor.rs:49`) returns `Arc<dyn Executor>` infallibly. **Confirmed** — it structurally cannot report "unknown executor", which is why AUD-014 exists.
- No `.github/` directory. **Confirmed** — no CI.
- Working tree at audit time: ~95 modified `.md` files with no substantive change. **Confirmed** — this is S0-2.

Not independently verified: the 206/371 test counts, and the specific Settings-tab and evidence-pipeline claims. Those rest on the audit's own live runs.
