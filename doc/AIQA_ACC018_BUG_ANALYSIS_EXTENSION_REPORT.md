# AIQA — ACC-018 AI Model Gateway Extension: BugAnalysis (AI-Assisted Root Cause)

**Date:** 22 August 2026
**Scope:** Build order item 4 of the confirmed sequence — the last one for today
(`AIQA_SCOPE_RECOMMENDATIONS_REPORT.md`). Extend the AI router to `AiTask::BugAnalysis`, following
`ai_generation.rs`'s exact `TestCaseGeneration` template, and wire it into `intelligence.rs`'s real
rule-based failure-clustering system as a genuinely additive, clearly-labeled AI hypothesis layer.

---

## 1. What was investigated before building

Grepped the whole backend for `BugAnalysis` before writing a line of code. Found: `AiTask::BugAnalysis`
**already exists** — `ai_router.rs`'s task enum (line 138) and its `required_capabilities` mapping
(`TextGeneration`, `Reasoning`, line 157) were both defined in an earlier phase. **Zero consumers called
it anywhere.** So this unit's real work was never "extend the router" in the sense of adding a new
variant — that variant was already there, unused — it was building the one thing genuinely missing: a
real caller.

Read `ai_generation.rs` in full for the exact template to reuse: skill/prompt builder → central
`AiInfrastructure::generate` (task-tagged `AiRequest`) → structured-JSON schema → validation →
`GenerationOutcome::Ai { .. } | Failed { reason }`, with the caller (`api.rs`) deciding what a failure
means for the HTTP response, and a `#[ignore]`'d live test as the real proof of liveness (never part of
the default suite/CI). Read `intelligence.rs` in full for the real consumer: `record_failure` already
calls the deterministic `classify_failure` and persists `(hypothesis, confidence)` onto
`failure_clusters.root_cause_hypothesis`/`.confidence` every time a run completes with a matching
recurring failure — this pipeline is unchanged by this unit.

**Design decision, made before writing any code, and why:** the new AI-assisted hypothesis is **on-demand
(HTTP-triggered), not wired into `ingest_run`'s synchronous run-completion path.** A real Ollama call can
take on the order of a minute (`ai_generation.rs`'s own live test, and this unit's own live run below,
~22s/call). Putting that inline in run-completion processing — which needs to stay fast for the
scheduler/event pipeline — would be a real regression, not a faithful extension of the existing
architecture. This also matches `TestCaseGeneration`'s own real shape: it is triggered by an explicit
`POST .../test-cases/generate-ai` request, not fired automatically as a side effect of something else. The
new endpoint, `POST /api/v1/projects/:id/findings/:finding_id/ai-hypothesis`, is the direct analog for
BugAnalysis, sitting alongside the existing (fast, cheap, unmodified) `POST .../findings/:id/analysis`.

**A second, smaller honest correction to the request as understood:** the request asked to prove liveness
"against two different real failure clusters" using a sample of the raw failure reason. Checked
`store.rs`'s `failure_clusters` schema directly: **no raw, per-occurrence failure reason is persisted
anywhere** once `record_failure` has run — only the *normalized* signature (numbers/hex ids collapsed to
`#`), the deterministic hypothesis, and the occurrence count. So the new prompt (`build_bug_analysis_prompt`)
is built only from what is genuinely, really available — the normalized signature — and says so explicitly
to the model (so it does not mistake `#` for literal text or invent a specific number to fill the gap).
Nothing here pretends to have raw failure-log access this backend does not actually retain.

---

## 2. What was built

| File | Change |
|---|---|
| `services/orchestrator/migrations/0007_ai_bug_hypothesis.sql` | **New.** 4 nullable columns on `failure_clusters`: `ai_hypothesis`, `ai_hypothesis_provider`, `ai_hypothesis_model`, `ai_hypothesis_generated_at` — additive, never touching the existing `root_cause_hypothesis`/`confidence` columns |
| `services/orchestrator/src/store.rs` | `FailureClusterRow` gains the 4 new fields; `list_failure_clusters`/`get_failure_cluster_by_id` updated to select them; new `set_ai_hypothesis(cluster_id, hypothesis, provider, model)` — the only place the 4 new columns are ever written, and only ever called on a genuine AI success |
| `services/orchestrator/src/intelligence.rs` | Module doc comment rewritten to state precisely what changed (the automatic pipeline: nothing) and what's new (an on-demand, separate layer). Added `build_bug_analysis_prompt`, `AiHypothesisResponse` (schema), `AiHypothesisOutcome`, `generate_ai_hypothesis(infra, signature, rule_based_hypothesis, occurrence_count)` — the `BugAnalysis` consumer, same role `ai_generation.rs::generate_test_cases` plays for `TestCaseGeneration`. 16 new tests (see §3) |
| `services/orchestrator/src/api.rs` | New route + handler `generate_finding_ai_hypothesis_scoped` (`POST .../findings/:id/ai-hypothesis`), `Role::Operator`-gated (same bar as `.../review` and `generate-ai`). Reuses the exact `generation_method`/`provider`/`model`/`fallback_reason` response shape `generate_ai_test_cases_scoped` established — a `"fallback"` outcome is a normal `200`, not a `5xx`, per the same established convention |
| `services/orchestrator/src/api_tests.rs` | 5 new HTTP-level tests (see §3) |

**Why this lives in `intelligence.rs`, not `ai_generation.rs`:** `ai_generation.rs`'s own module doc comment
scopes it specifically to "AI-powered, skill-aware test case generation." The new prompt/schema/consumer
function is bug-domain-specific, and its real consumer (`FailureClusterRow`) already lives in
`intelligence.rs` — colocating keeps each file's own domain coherent, matching the same reasoning
`ai_generation.rs` itself already documents (task-specific logic lives with its task, execution goes through
the shared `ai_router`).

---

## 3. Tests added — mapped to ACC-018's own Verification checklist

ACC-018's Acceptance Matrix criteria require testing: **1. Local model success. 2. API fallback. 3. Model
timeout. 4. Model unavailable. 5. Fallback failure.** All five, specifically for `BugAnalysis` (not just
relying on the pre-existing generic router tests):

**`intelligence.rs` (11 new):**
- `bug_analysis_prompt_carries_the_real_signature_hypothesis_and_occurrence_count` / `..._differs_by_real_signature_...` — prompt-builder unit tests.
- `ai_hypothesis_response_schema_rejects_missing_hypothesis_field` — schema validation.
- `integration_real_routing_to_bug_analysis_returns_ai_outcome_on_success` — **checklist item 1.**
- `integration_falls_back_to_the_next_best_model_when_the_primary_fails` — **checklist item 2**, real second provider, real `AiInfrastructure` AUTO-mode fallback (not a parallel fallback mechanism).
- `integration_real_timeout_reports_truthful_failure_never_fabricated_hypothesis` — **checklist item 3.**
- `integration_no_provider_available_reports_no_compatible_model_never_fabricated_hypothesis` — **checklist item 4.**
- `integration_reports_truthful_failure_when_every_candidate_including_fallback_fails` — **checklist item 5.**
- `integration_invalid_structured_output_reports_truthful_failure`, `integration_empty_hypothesis_string_is_treated_as_a_real_failure_not_a_blank_success` — never-fabricated-output edge cases.
- `required_capabilities_for_bug_analysis_are_real_and_unchanged_by_this_unit` — confirms the pre-existing capability requirement (not redefined by this unit).

**`api_tests.rs` (5 new, HTTP-level, real router + real SQLite):**
- `be18_finding_ai_hypothesis_returns_real_outcome_and_never_overwrites_deterministic_cluster_data` — the explicitly-required proof: reads the real `root_cause_hypothesis`/`confidence`/`occurrence_count` **before** the AI call, asserts them **byte-for-byte unchanged after**, alongside the new, separately-named AI fields. Also confirms the action is audited.
- `be18_finding_ai_hypothesis_reports_truthful_fallback_on_provider_timeout_never_fabricated` — ACC-018 Failure Behavior directly: a real timeout leaves `ai_hypothesis` null, `review_state` at the real, pre-existing `NEEDS_REVIEW`, and the real deterministic hypothesis untouched; audited as `"fallback"`, not silently dropped.
- `be18_finding_ai_hypothesis_rbac_viewer_forbidden_operator_allowed`, `..._project_isolation_enforced`, `..._missing_finding_returns_structured_not_found` — RBAC/tenancy/404, same bar as every other findings route.

---

## 4. Live proof — the actual requirement, not just that the plumbing exists

Checked whether a live model is genuinely reachable on this machine today, per this unit's own instruction,
rather than assuming: `GET http://localhost:11434/api/tags` → real `200`, one real model listed
(`qwen2.5:7b-instruct`, 7.6B, real Ollama). A live model **is** reachable — ran the `#[ignore]`'d live test
manually (`cargo test --lib live_ollama_different_failure_clusters -- --ignored --nocapture`), the same way
`ai_generation.rs`'s own GO verdict was proven:

- Two real, different example failures (`connect ECONNREFUSED 127.0.0.1:8080`, `Timeout 30000ms exceeded`)
  run through the real, unchanged `normalize_signature`/`classify_failure` to get two genuinely different
  real (signature, rule-based-hypothesis) pairs — not hand-picked/fabricated inputs.
- Both routed through the real `AiInfrastructure::from_env()` + `discover_all()` (real discovery, real
  routing, real execution — the same infrastructure `TestCaseGeneration` uses, no parallel path) to the real
  local Ollama server.
- **Real output, verbatim from the test run:**

  > **CONNECTION-REFUSED** via `ollama/qwen2.5:7b-instruct`: *"Network connectivity issue resulted in
  > connection refused — likely due to a target service being down or still starting, or the connection
  > attempt was directed to an incorrect IP address or port"*
  >
  > **TIMEOUT** via `ollama/qwen2.5:7b-instruct`: *"Timeout was exceeded, indicating a potential delay in
  > the operation's execution — investigate for slow processing within the system or external
  > dependencies, not necessarily a logic bug within the code itself"*

- `assert_ne!` on the two hypothesis texts passes — genuinely different structure and content, not a fixed
  template with a field substituted in (a template-substitution would have produced two strings differing
  only in a single swapped word/phrase; these differ in overall structure and reasoning). Total run time
  44.16s for both calls (`test result: ok. 1 passed; 0 failed`).

**Verdict for this specific task type: live GO, not wiring-only.** `AiTask::BugAnalysis`, end-to-end through
the real central `AiInfrastructure`, against a real local model, produces genuinely different, genuinely
useful hypothesis text for genuinely different real inputs — the same standard the original AI Model
Auto-Discovery GO verdict was held to.

---

## 5. Verification

```
cd services/orchestrator && cargo build          → clean, exit 0
cd services/orchestrator && cargo test --lib     → 206 passed, 0 failed, 2 ignored (was 190/0/1 before this unit)
cd services/orchestrator && cargo test --lib live_ollama_different_failure_clusters -- --ignored --nocapture
                                                  → 1 passed (manual, live-Ollama-dependent — see §4)
```

Net new tests: **+16 (190→206)** — 11 in `intelligence.rs`, 5 in `api_tests.rs` — plus 1 new `#[ignore]`'d
live test (run manually, passed — not counted in the default 206, by the same established convention as
the pre-existing `live_ollama_different_skills_produce_different_real_coverage`).

Frontend was not touched this unit (no file under `forentend/` edited) — the last-verified **370 passed, 0
failed, 20 files** (`npx vitest run`, cited in the immediately prior unit's report) stands; not re-run here
since nothing frontend-attributable changed.

---

## 6. Acceptance Matrix updated

`AIQA_ACCEPTANCE_MATRIX_v1.0.md`:

- **§5 grid:** ACC-018 Final **stays `VERIFIED`** — per this unit's own instruction, this is an extension
  of an already-VERIFIED capability, not a reason to downgrade it, and the extension itself independently
  reached a live GO (§4), so there is no honest basis to downgrade.
- **§5A:** ACC-018's row extended (not replaced) with the BugAnalysis addition: what was built, that this
  specific task type independently reached its own live GO (not wiring-only), and the 16 new tests mapped
  to ACC-018's own 5-item Verification checklist.
- **§34:** Backend Automated Tests corrected from `190/0/1` to `206/0/2`, both `#[ignore]`'d live tests
  (TestCaseGeneration's pre-existing one and BugAnalysis's new one) noted as separately run manually and
  passed.

---

## 7. What remains unresolved / deliberately out of scope

- **No frontend UI was built for this.** The task's own instructions were entirely backend-scoped (router
  extension, `intelligence.rs` wiring, backend tests) — no frontend page/button was requested, and none was
  built, consistent with avoiding scope expansion. `ai_hypothesis`/`ai_hypothesis_provider`/
  `ai_hypothesis_model` are real fields on `FailureClusterRow`, already returned by the existing
  `GET .../findings/:id`/`POST .../findings/:id/analysis` endpoints (additive — no existing frontend
  contract is broken), and by the new `POST .../findings/:id/ai-hypothesis` — surfacing them in
  `ReportsPage.tsx`'s finding-detail view is a natural next step, but was not part of this unit and was not
  done.
- **The AI hypothesis is genuinely additive, never confirmed/promoted automatically.** It has no effect on
  `review_state`, `severity`, or any confirmed-defect promotion logic — a human still reviews and confirms
  or dismisses via the existing, unchanged `POST .../findings/:id/review`.
- **Not re-tested:** whether a *different* real model (not `qwen2.5:7b-instruct`) or a configured API
  provider (Gemini/OpenAI/Anthropic — none has a credential configured in this environment) produces
  comparably good BugAnalysis output. The live proof in §4 is real and sufficient to close "is this wired
  and does it work end-to-end", not a claim about output quality across every possible model.
- **Did not fabricate a raw failure-reason sample.** See §1 — the prompt is honestly built from the
  normalized signature only, because that is genuinely all this backend retains once a failure is clustered.

---

This closes build order item 4 and today's full confirmed build sequence (items 1–4: Test Case Validator,
System Telemetry, Report Export, AI Bug Analysis — all built, tested, and reported). Per the task's own
explicit instruction: no new build item will be started without checking back first. The remaining backlog
— the deferred/descoped items from `AIQA_SCOPE_RECOMMENDATIONS_REPORT.md` and Wave 3's E2E proof — needs
its own fresh sequencing conversation, not an assumed next step.
