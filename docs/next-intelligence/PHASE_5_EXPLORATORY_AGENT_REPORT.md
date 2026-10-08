# Phase 5 — AI Exploratory Testing Agent

**Phase 5 of `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`. Backend only. PASS.**

Predecessors: [`PHASE_4_PASS.md`](./PHASE_4_PASS.md), and Phases 1–4 generally. None were
restarted or rewritten.

---

## 1. Audit: what exploratory capability already exists

The decisive finding is that **a bounded agent runtime already exists, is well designed, is
already in production use, and has simply never been pointed at exploration.**

| Existing component | What it provides | Reused? |
|---|---|---|
| `agent.rs` — `Agent` trait, `run_agent_loop`, `AgentLimits`, `AgentContext`, `AgentTraceHook` | the bounded reasoning loop: `max_turns`, `max_tool_calls_total`, `max_observation_bytes`, `timeout`, cooperative cancellation, project authorisation | **yes, unchanged** |
| `agent_tools.rs` — `AgentTool`, `ToolSafetyLevel` (`ReadOnly`/`Diagnostic`/`Mutating`/`Dangerous`, `is_autonomous_safe()`), `ToolDefinition::validate()`, `AgentToolRegistry` | the tool contract and safety classification | **yes, unchanged** |
| `tools/` — `schema_inspector`, `dom_prober`, `requirement_lookup`, plus `security.rs` (path guard, secret sanitiser) | three production diagnostic tools | **yes, unchanged** |
| `agents/healing.rs` — `SelfHealingAgent`; `agents/planning.rs` — `TestPlanningAgent` | two production `Agent` implementations | **precedent followed** |
| `governed_healing::auto_diagnose` | drives a real agent through `run_agent_loop` in a live request | **precedent followed** |
| `execution_plan::Action` | 15 variants — the action vocabulary | **yes, unchanged** |
| `execution_plan::validate_plan` | structural validation + **SSRF guard** + secret-reference guard | **yes, called directly** |
| `policy::evaluate` → `PolicyDecision` | `Deny` / `RequireApproval` / `Allow`, from `blocked_executors` and `require_approval_for` | **yes, called directly** |
| `evidence::LocalEvidenceStore` | content-addressed, traversal-guarded artifact storage | **yes, unchanged** (one `Debug` derive added) |
| `canonical_failure_fingerprint` (Phase 2) | the one fingerprint space | **yes — the dedup key** |
| `crawler::CrawlOutput` / `RoleCrawlResult` / `CrawledPage` / `CrawledElement` / `VisibilityDiffEntry` | real crawl evidence, per-role pages, the visibility matrix | **yes — the exploration surface** |
| `QaMemoryReader::prioritise_tests` (Phase 4) | the risk / history signal | **yes, read** |
| `ExecutorRegistry` + `SimulatedExecutor` + `PlaywrightExecutor` + … | execution | **driven, never reimplemented** |

**`agent_tests.rs` has ~20 tests and `governed_healing` runs an agent in production. Nothing
was missing except an agent that explores.**

### 1.1 Four findings from the audit that shaped the design

1. **`execution_telemetry.assertion_result` exists and nothing ever fills it.** The column is
   read by `store.rs` and written as `None` by the only construction site that sets it; the
   API path builds its own `ApiAssertionResult` for the run log rather than for telemetry. So
   every telemetry row this system produces — including from a real browser executor — has
   `assertion_result IS NULL`. Phase 5 consults the column *first* and falls back to a
   declared executor report, so the structural signal takes over automatically if a future
   executor starts filling it. Reported as L1.

2. **The Action AST has no browser-history variant.** The phase brief asked for back/forward
   navigation. There is no `Action` for it, and adding one would be a second Action AST.
   `behaviour_support_report()` returns it as `UNSUPPORTED` with a reason rather than
   silently skipping it (§4.2).

3. **There is no refresh action either.** `Refresh` is reported as `APPROXIMATED` — the agent
   re-navigates to the same URL and says so — never as a refresh.

4. **`validate_target_url` permits loopback only on port 8080** (`ORCHESTRATOR_PORT`), by
   explicit design "which is what makes a local dev server testable at all". The tests use
   that port rather than reaching for `AIQA_ALLOW_PRIVATE_TARGETS`, so the SSRF guard stays
   exactly as strict in tests as in production. The SSRF negative test uses
   `169.254.169.254`, which the guard rejects on port grounds not covered by that exception.

---

## 2. Architecture

```
ExplorationService::explore
  │
  ├─ Phase 4 QaMemoryReader::prioritise_tests ──── risk/history (read once, absent ⇒ None)
  ├─ store::insert_exploration_session ─────────── the session row + its budget
  │
  └─ run_agent_loop(agent, context, registry, ai, trace)      ← the EXISTING loop
       │
       ├─ ExploratoryAgent::step()   stateless; reads context.history only
       │    observe → hypothesize → act → verify → classify → continue/stop
       │
       └─ registry (the EXISTING AgentToolRegistry)
            ├─ observe_surface   ReadOnly   — the crawl surface, nothing else
            ├─ recall_history    ReadOnly   — risk signals + known fingerprints
            └─ probe_action      Diagnostic — the only one that acts:
                 1. project scope + is_authorized_for_project
                 2. BUDGET → refuse
                 3. behaviour must be in the catalogue
                 4. safety level must be autonomously safe
                 5. ≥1 assertion required
                 6. execution_plan::validate_plan   ← real SSRF + secret guard
                 7. policy::evaluate                ← real blocked/gated check
                 8. RunPipelineProbeRunner::run     ← the existing run pipeline
                 9. classify → capture evidence → persist with dedup
```

### 2.1 The agent composes; it does not execute

`agents/exploration.rs` contains **no execution code at all**. That is the property that makes
"no second browser executor" checkable rather than aspirational: the one place a probe reaches
an executor is `agents/exploration_execution.rs`, and it is nothing but
`insert_run_for_release` → `Intelligence::ingest_run` → read the telemetry back.

Reading the telemetry back is deliberate. The fingerprint and the cluster link are computed
from **what was recorded**, not from what the dispatcher claimed, so if the two ever disagree
the database wins.

### 2.2 The agent is stateless, exactly as `SelfHealingAgent` is

`Agent::step` takes `&self`. The agent's whole memory is `context.history`, which the runtime
fills with every `TurnRecord`. Its run accounting lives in a shared `SharedCounters` mutex —
that is bookkeeping, not reasoning, and it is what lets the tool enforce the budget
independently of the agent.

---

## 3. Bounded by construction, twice over

`run_agent_loop` already enforces `max_turns`, `max_tool_calls_total`, `max_observation_bytes`
and `timeout`, and honours cancellation. On top of that:

| Ceiling | Default | Enforced in the agent | Enforced in the tool |
|---|---|---|---|
| `max_turns` | `AgentLimits` | loop terminates | — |
| `max_tool_calls` | `AgentLimits` | — | — |
| `timeout` | `AgentLimits` | loop terminates | — |
| `max_actions` | 40 | `step` stops choosing work | **`probe_action` refuses** |
| `max_state_transitions` | 20 | `step` stops choosing work | **`probe_action` refuses** |
| `max_findings` | 10 | `step` stops choosing work | **`probe_action` refuses** |
| `max_pages` | 8 | `step` stops choosing work | **`probe_action` refuses** |

The second column of the last block is the one that matters for safety: *an agent that
miscounts is a bug; a tool that acts anyway is a vulnerability.* `every_budget_dimension_stops_exploration`
and `the_tool_refuses_to_act_once_the_budget_is_spent` prove both, the latter by pushing the
tool four times past a one-action budget and watching the counter stay at 1.

A budget of zero stops exploration immediately and is **not** raised to a default.

Every stop is named — `budget_exhausted: max_actions`, `catalogue_exhausted`, `no_surface`,
`limits_reached`, `cancelled` — and is recorded verbatim on the session row. "It stopped" with
no reason is the outcome a reader cannot act on.

---

## 4. Action and state model

### 4.1 Behaviours, and what the AST supports

| Requested behaviour | Support | Carried out with |
|---|---|---|
| intelligent navigation | **Native** | `Navigate` + `AssertUrl` |
| multi-step state transitions | **Native** | ordered sequences |
| unexpected navigation | **Native** | `AssertUrl` on the current route; a mismatch is the finding |
| duplicate actions/clicks | **Native** | the same `Click` twice |
| repeated submission | **Native** | `Click` ×2 then `AssertVisible` |
| boundary values | **Native** | `Fill` (1024 chars) + `AssertEnabled` |
| invalid values | **Native** | `Fill` (markup-bearing) + `AssertEnabled` |
| empty values | **Native** | `Fill` (`""`) + `AssertEnabled` |
| session expiry | **Native** | `Navigate` + `AssertVisible` |
| role / state changes | **Native** | `AssertVisible` against an element the visibility matrix records as role-restricted |
| refresh | **Approximated** | `Navigate` to the same URL — **no refresh action exists** |
| **back / forward** | **UNSUPPORTED** | **no browser-history variant exists** |

The last row is the honest answer to a request the schema cannot express. `behaviour_support_report()`
returns it, and `the_behaviour_report_states_what_the_action_ast_cannot_do` asserts it.

Every probe is composed from **crawled elements only**. An element the crawler never saw is
not a target, because acting on it would mean acting on a selector nobody has evidence exists.
`the_surface_contains_only_what_the_crawl_recorded` pins this.

### 4.2 Prioritisation is evidence-driven, not random

Within the catalogue, the agent tries behaviours it has not yet tried and picks the first
unvisited route in a deterministic order. There is **no randomisation**, which makes a session
reproducible and a change in its findings visible.

Risk does **not** reorder routes, and the code says why: Phase 4 ranks *test cases*, and this
schema has no `covers` edge, so no test case can be attributed to a route. Mapping a route's
risk would mean guessing which tests exercise it — the invented relationship this codebase
refuses to record. Risk is reported to the caller (`recall_history`) and justifies *why* a
session was run.

Where data is unavailable it is absent, not zero: a project with no history yields
`prioritisation: null` with `has_prioritisation: false`, never an empty low-risk ordering.
`risk_history_is_reported_and_its_absence_is_not_low_risk` asserts both halves.

---

## 5. Finding classification: evidence before verdict

| Class | Requires | Confidence |
|---|---|---|
| `CONFIRMED_DEFECT` | a recorded failing execution **and** a real canonical fingerprint **and** a stated expectation **and** a covering requirement | 0.9 |
| `POTENTIAL_DEFECT` | evidence + fingerprint, but missing either the expectation or the requirement | 0.5–0.6 |
| `OBSERVED_ANOMALY` | evidence + fingerprint, but neither expectation nor requirement | 0.4 |
| `INSUFFICIENT_EVIDENCE` | **anything less** | **0.0** |

Declared weakest-first, so the derived `Ord` *is* evidence strength and `<` reads the way the
name suggests. Every finding carries the `basis` clause that established it — the evidence,
not the verdict — and that travels into the database and out through the API.

`INSUFFICIENT_EVIDENCE` is reached by five distinct routes, each with its own stated reason:

1. the plan was refused before it ran;
2. no executor evaluated the assertions;
3. the assertions held (the overwhelmingly common case — a clean probe returns no finding);
4. a failure was recorded but no fingerprint could be computed;
5. *(and it is the default a caller falls back to)*

`missing_evidence_never_becomes_a_confirmed_defect` checks all of them;
`each_missing_input_caps_the_classification` shows that omitting any single input drops the
class one rung and that no omission reaches `CONFIRMED_DEFECT`.

The one input that must not be trusted is a self-report. `assertions_evaluated` comes from the
recorded `assertion_result` **or** the dispatcher's declared executor report — never from the
agent's own reasoning.

---

## 6. Duplicate suppression

The key is Phase 2's `canonical_failure_fingerprint`, computed from the recorded
`error_message`. Because the fingerprint is derived from category + masked reason + selector +
endpoint and **not** from the path taken, the same underlying defect reached through a
different route, a different action order, or a different session produces the same key.

Suppression is enforced **structurally**, by a partial unique index:

```sql
CREATE UNIQUE INDEX uq_exploration_findings_defect
    ON exploration_findings(project_id, failure_fingerprint)
    WHERE failure_fingerprint IS NOT NULL;
```

so "the same underlying defect" is a property of the schema rather than of application code
remembering what it has seen. The writer uses `ON CONFLICT DO NOTHING` and then re-reads, so
the second report becomes a recorded **suppression naming what it duplicated** — the
difference between a system that deduplicates and one that loses information.

NULLs are deliberately **not** folded into the index. An `INSUFFICIENT_EVIDENCE` row is not a
claim about the application and must not stop the same inconclusive observation being recorded
in a later session; `COALESCE` here would suppress real observations, the opposite of the
requirement. `observations_without_a_fingerprint_are_not_deduplicated` pins that.

A matching `failure_clusters` row is resolved and attached as `related_failure`, so an
exploration finding joins the one existing failure history rather than starting a second one.

---

## 7. Safety and governance

Every probe passes through, in order:

| # | Gate | Source |
|---|---|---|
| 1 | project scope present, then `is_authorized_for_project` | existing `AgentContext` |
| 2 | budget not exhausted | new, enforced twice |
| 3 | behaviour in the catalogue | new |
| 4 | `required_safety().is_autonomous_safe()` | existing `ToolSafetyLevel` |
| 5 | ≥1 `Assert*` in the probe | new |
| 6 | `validate_plan` — structure, secrets, **SSRF** | existing, called directly |
| 7 | `policy::evaluate` — blocked / gated executors | existing, called directly |
| 8 | run through `insert_run_for_release` + `ingest_run` | existing writers |

No arbitrary code execution, no shell, no direct database writes outside the two Phase 5
tables, and no second Action AST, browser executor, evidence system or agent runtime.

**Destructive actions:** every catalogue entry is `Diagnostic` and therefore autonomously safe.
A destructive technique is *not in the catalogue*, and the budget does not grow to accommodate
one. If one were added it must be declared `Mutating` or `Dangerous`, which makes
`is_autonomous_safe()` false and requires an operator to opt in —
`every_catalogue_entry_declares_its_support` asserts the current state.

A gated executor is **not** run autonomously: `a_gated_executor_is_not_run_autonomously`
shows it runs, and the finding classifies `INSUFFICIENT_EVIDENCE` with "requires human
approval" as its basis.

---

## 8. Evidence handling

`ExplorationFinding::from_observation` writes the artifact and captures the key in one place,
so a finding cannot exist without the object key that supports it — and the key is always one
that was actually written. Content: behaviour, route, the **real serialized `Action` AST**,
observed, expected, violated requirement, status, failure reason, fingerprint, app state,
classification, basis, whether assertions were evaluated, and any refusal.

Bytes live in `LocalEvidenceStore`; the row holds only the key — never inlined into a DB row,
never handed to a model.

`a_recorded_finding_carries_readable_evidence` reads the bytes back, parses them, and asserts
the stored row's `action_sequence` is the same sequence as the evidence's.

---

## 9. Files changed

| File | Change |
|---|---|
| `agents/exploration.rs` | **new** — budget, counters, behaviour catalogue, `ExplorationSurface`, `ProbeObservation::classify`, `ExplorationFinding`, `ProbeRunner` seam, `ProbeActionTool`, `ObserveSurfaceTool`, `RecallHistoryTool`, `ExploratoryAgent`, `ExplorationService`, `behaviour_support_report` |
| `agents/exploration_execution.rs` | **new** — `AssertionEvidence`, `DispatchOutcome`, `RunPipelineProbeRunner` |
| `exploration_tests.rs` | **new** — 29 tests + 1 `#[ignore]`d measurement |
| `migrations/0032_exploration.sql` | **new** — `exploration_sessions`, `exploration_findings`, 3 indexes, 1 partial unique index |
| `agents/mod.rs` | register + re-export the new modules |
| `store.rs` | 2 row DTOs, 7 writers/readers, 2 row mappers — all additive |
| `evidence.rs` | `+ Debug` on `LocalEvidenceStore` (it is a `PathBuf`), needed by the `AgentTool` bound |
| `api.rs` | one read-only route + handler |
| `lib.rs` | test module registration |
| `trace_tests.rs`, `trace_chain_tests.rs`, `change_impact_tests.rs` | migration-count guards 31 → 32 |

### 9.1 Migration 0032

Two brand-new tables. Nothing altered, dropped or recreated, so it is not destructive. A
partial unique index supplies duplicate suppression, and `project_id` is a column rather than a
join for the reason Phase 3's 0031 gives: a forgotten `WHERE` on the one table whose reads are
constantly project-scoped is a cross-tenant leak rather than an empty result.

The three existing migration-count guards fired and were moved to 32 **with a comment naming
the migration and why it is additive** — not silenced. That they fired is the mechanism
working.

### 9.2 API

```
GET /api/v1/projects/:project_id/exploration[?classification=&limit=]
```

Read-only and **deliberately so**: starting an exploration dispatches probes to a real
executor, which is not something a `GET` should do. Returns sessions, findings,
`finding_counts`, and `behaviour_support` (so "can you test back/forward?" gets an explicit
`UNSUPPORTED`). An unknown `classification` is refused with `validation` rather than silently
returning nothing. Guarded by `require_scoped_project`.

---

## 10. Tests

**29 tests + 1 `#[ignore]`d measurement.** Over half are negative.

| Requirement | Test |
|---|---|
| bounded exploration | `every_budget_dimension_stops_exploration`, `the_default_budget_is_bounded_on_every_dimension`, `a_zero_budget_stops_immediately_rather_than_defaulting` |
| normal successful exploration | `a_session_runs_within_its_budget_and_records_why_it_stopped` |
| meaningful anomaly detection | `each_missing_input_caps_the_classification`, `evidence_and_a_covering_requirement_produce_a_confirmed_defect` |
| **insufficient-evidence handling** | `missing_evidence_never_becomes_a_confirmed_defect`, `the_production_default_cannot_produce_a_confirmed_defect` |
| **duplicate finding suppression** | `a_known_defect_reached_again_is_suppressed_and_reported`, `observations_without_a_fingerprint_are_not_deduplicated`, `findings_are_retrievable_by_fingerprint_for_the_suppression_report` |
| **action-policy enforcement** | `a_blocked_executor_stays_blocked`, `a_gated_executor_is_not_run_autonomously`, `a_plan_the_real_validator_refuses_is_not_executed` |
| **project isolation** | `a_probe_cannot_cross_a_project_boundary`, `exploration_without_a_project_scope_is_refused` |
| **exploration stop condition** | `a_session_runs_within_its_budget_and_records_why_it_stopped`, `an_empty_surface_stops_without_inventing_a_target` |
| evidence persistence | `a_recorded_finding_carries_readable_evidence` |
| **historical-memory / risk usage** | `risk_history_is_reported_and_its_absence_is_not_low_risk` |
| **invalid / unsafe action rejection** | `an_unknown_behaviour_is_rejected_rather_than_guessed`, `a_probe_with_no_assertion_is_refused_rather_than_run` |
| **repeated exploration, no unbounded duplicates** | `repeated_exploration_does_not_produce_unlimited_duplicates` |
| cannot exceed budget | `the_tool_refuses_to_act_once_the_budget_is_spent` |
| cannot loop forever | `exploration_cannot_loop_forever` |
| surface is crawl-derived | `the_surface_contains_only_what_the_crawl_recorded`, `element_roles_are_derived_from_the_recorded_kind` |
| behaviour support honesty | `the_behaviour_report_states_what_the_action_ast_cannot_do`, `every_catalogue_entry_declares_its_support` |
| ladder shape | `the_classification_ladder_is_ordered_by_evidence` |

All fixtures use real writers: `insert_test_case`, `insert_run_for_release`,
`Intelligence::ingest_run`, `insert_exploration_session`, `LocalEvidenceStore::put`,
`ProbeActionTool::execute` → `record_exploration_finding`. `CrawlOutput` is built through its
own serde contract — the same parse `read_crawl_result` performs on the TypeScript side's
file. Nothing hand-writes a fingerprint, a risk figure or a row the real system could not
produce.

### 10.1 Bugs found and fixed

1. **`BehaviourSupport` could not be serialized.** `#[serde(tag = "kind")]` cannot flatten a
   newtype variant's payload. Caught by `the_behaviour_report_states_what_the_action_ast_cannot_do`,
   because the report is built with `json!`. Fixed with `content = "detail"`. A type that
   cannot serialize is a type that cannot be reported over HTTP.
2. **`FindingClass`'s derived order contradicted its meaning.** The enum was declared
   strongest-first, so `<` meant "weaker" — the opposite of what a caller would read.
   Reordered weakest-first so the derived `Ord` **is** evidence strength.
3. **Two live bugs I introduced while wiring this up**, both caught by the compiler, not by
   inspection: a splice that consumed `impl Store`'s closing brace in `store.rs`, and a regex
   edit that moved the row mappers *inside* the `impl`. Both repaired by brace-depth
   verification rather than by eye — the lesson from Phase 3's `store.rs` corruption, applied
   before it could bite.
4. **My own fixtures were in the wrong wire format.** `CrawlOutput` is `camelCase` and `Action`
   is `#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]`. Both fixtures were written
   in the shape I assumed rather than the shape the crawler produces. Caught on first run, and
   fixed *to* the real format rather than by relaxing the structs — so the tests now prove the
   real wire form round-trips.
5. **A foreign key caught six fixtures.** `exploration_findings.session_id` references
   `exploration_sessions`, so a finding cannot exist without the session that produced it. Six
   tests were creating tools without a session; the constraint did its job and the fixtures
   were fixed to open one through the real writer.

Nothing was weakened, skipped or deleted to make a test pass.

---

## 11. Performance

Measured, not asserted. Reproduce with:

```
cargo test phase5_measures_exploration_cost -- --ignored --nocapture
```

Dataset: 40 test cases × 3 executions = 120 telemetry rows, built through the real
`ingest_run` path in 279 ms.

| Cost | Measured |
|---|---|
| **memory / risk retrieval** — Phase 4 prioritisation over 120 rows | **9 ms**, 40 cases ranked |
| **evidence collection** — 50 artifact writes | **31 ms** |
| **duplicate detection** — 50 same-fingerprint inserts (49 suppressed) | **7 ms** |
| **exploration** — a whole bounded session | **6 actions in 55 ms** |

### 11.1 Observations

* **Duplicate detection is the cheapest path, by design.** 7 ms for 50 attempts means the
  unique index absorbs almost the entire cost — the rejected inserts never reach a row. This
  matters because suppression runs on *every* finding.
* **Evidence dominates the per-finding cost** (0.6 ms per write, dominated by `create_dir_all`
  plus a fsync-adjacent write), which is the price of the requirement that a finding carry
  readable proof.
* **Risk retrieval is read once per session**, not once per probe, following Phase 4's
  `TestEvidence` pattern: `explore` calls `prioritise_tests` once and hands the result to the
  agent. Without that it would be one full telemetry scan per probe.
* **The session stopped at `CatalogueExhausted` after 6 actions**, not at its budget: several
  catalogue entries cannot be composed for a two-route fixture with one form and one link, and
  each such behaviour consumes a turn before being marked tried. That is bounded and correct
  but not efficient — see L6.
* No timing threshold is pinned in CI. The measurement is `#[ignore]`d precisely because a
  wall-clock assertion chosen on one machine fails on another; the numbers live here and the
  test exists so they can be reproduced.
* All figures are from a freshly built, fully migrated **test** database on this machine. They
  are not a production latency claim.

---

## 12. Regression

| Run | Before Phase 5 | After |
|---|---|---|
| Backend suite | 1909 − 29 = **1880** passed / 0 failed / 3 ignored | **1909 passed / 0 failed / 4 ignored** |
| `cargo build` | clean | clean |
| `cargo clippy --all-targets` | 0 errors | 0 errors, no warning in any Phase 5 file |
| `rustfmt --check` | — | clean on all 5 Phase 5 files |
| Frontend | 1605 / 79 files | unchanged — no frontend file touched |

Zero regressions. Phase 1–4 suites all still pass individually: `trace_tests` 21,
`trace_chain_tests` 14, `change_impact_tests` 16, `risk_prioritisation` 18. The 4th ignored
test is this phase's measurement.

---

## 13. Limitations

**L1 — `assertion_result` is never populated.** A real telemetry column with no writer. Until
an executor fills it, "assertions were evaluated" can only be *declared* by the dispatcher,
which is why `AssertionEvidence::NotEvaluated` is the default and every default-path finding
is `INSUFFICIENT_EVIDENCE`. The column is consulted first, so filling it is a one-sided change
with no edit here.

**L2 — no browser executor in this environment.** `Action::Assert*` semantics are decided by
the Playwright executor in its own TypeScript process. Without node plus a browser, a probe can
observe pipeline behaviour but cannot confirm application defects. The classification ladder is
therefore exercised two ways: the production default path is proven to yield **no** confirmed
defect, and a declared executor report stands in for the browser to prove the ladder *can* be
climbed. That double is a test double and is labelled as one.

**L3 — back/forward navigation is unsupported.** No `Action` variant exists and adding one
would be a second Action AST. Reported as `UNSUPPORTED` rather than approximated.

**L4 — refresh is approximated** by re-navigating, and every finding records the behaviour
name so a reader knows which technique actually ran.

**L5 — exploration cannot be steered to a route by risk.** Phase 4 ranks test cases and there
is no `covers` edge, so no case can be attributed to a route. Risk is reported and justifies
why a session ran; it does not reorder routes. Inherited from Phase 2 L1 / Phase 4 L1.

**L6 — a behaviour that cannot be composed consumes a turn.** Honest (it distinguishes "we
tried nothing here" from "we tried and found nothing") but wasteful: a session can exhaust
`max_turns` on non-composable behaviours rather than on probes. Bounded, therefore safe, but a
future version should skip a behaviour that the remaining routes cannot support.

**L7 — no requirement linkage.** `violated_requirement` is supplied by the caller and is not
yet populated from `requirement_lookup`, so in practice most findings cap at `POTENTIAL_DEFECT`
or `OBSERVED_ANOMALY`. Wiring the existing `requirement_lookup` tool to it is the obvious next
step and needs no schema change.

**L8 — the surface is only as good as the last crawl.** Exploration acts on `CrawlOutput`, so
anything the crawler missed is not explorable. Stated as the bound, not worked around.

**L9 — the live database is unchanged** and has not had migration 0032 applied. Every figure in
this report comes from freshly built, fully migrated test databases driven through real
writers. None of it is a claim about production data.

**L10 — no E2E CI job.** Pre-existing; `ci.yml` has three jobs, none E2E.

---

## 14. PASS gate

| Criterion | Status | Evidence |
|---|---|---|
| exploratory execution is genuinely functional | **PASS** | a real `Action` plan validated by the real validator, gated by the real policy, executed through the real run pipeline, producing real telemetry and real fingerprints — end to end in 7 tests |
| exploration is bounded | **PASS** | 4 budget ceilings enforced twice (agent + tool) plus 4 runtime ceilings; 3 tests, one of which pushes the tool 4× past its limit |
| all actions remain governed | **PASS** | 8 gates per probe; blocked, gated, SSRF, unscoped, unknown-behaviour and no-assertion each rejected in their own test |
| findings require evidence | **PASS** | 4-rung ladder with `INSUFFICIENT_EVIDENCE` as the floor; 3 tests, one checking 5 distinct evidence gaps |
| duplicate findings are controlled | **PASS** | enforced by a partial unique index on Phase 2's fingerprint; 3 tests incl. the NULL-not-folded case |
| project isolation passes | **PASS** | 2 tests: cross-project probe + an unscoped probe refused |
| risk/history can guide exploration where evidence exists | **PASS** | Phase 4's prioritisation read once per session and surfaced; absence reported as absence |
| tests pass | **PASS** | 29/29 |
| regression passes | **PASS** | 1909 / 0 failed; Phases 1–4 suites individually green |
| documentation complete | **PASS** | this report |

### Honest caveats

1. **The most important thing this phase produced is a negative result:** without a
   browser-backed executor that reports per-assertion outcomes, exploration can observe the
   pipeline but **cannot confirm a defect**. That is now enforced by a test rather than left as
   a caveat in prose.
2. **One requested technique is unsupported** (back/forward) and one is approximated (refresh),
   because the Action AST cannot express them. Reported, not faked.
3. **Four of the requested signals are unavailable** — requirement criticality, module risk,
   coverage gaps and per-test release trend — for the same structural reasons Phase 4
   documented, and inherited unchanged.

**Phase 5 = PASS**, with L1–L3 carried forward as the honest bound of what exploration can
currently claim.

---

**Phase 6 not started. Nothing committed. Nothing pushed.**
