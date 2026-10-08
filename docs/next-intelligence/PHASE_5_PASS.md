# Phase 5 — PASS

**Phase 5 of `AIQA_NEXT_INTELLIGENCE_PHASE_PLAN.md`: AI Exploratory Testing Agent.** Full
evidence in [`PHASE_5_EXPLORATORY_AGENT_REPORT.md`](./PHASE_5_EXPLORATORY_AGENT_REPORT.md).

Predecessor: [`PHASE_4_PASS.md`](./PHASE_4_PASS.md) — Phases 1–4 not restarted or rewritten.

---

## Result

**PASS.**

| Criterion | Status | Where proven |
|---|---|---|
| exploratory execution is genuinely functional | PASS | real `Action` plan → real `validate_plan` → real `policy::evaluate` → real run pipeline → real telemetry and real fingerprints |
| exploration is bounded | PASS | 4 budget ceilings enforced in *both* the agent and the tool, plus 4 runtime ceilings |
| all actions remain governed | PASS | 8 gates per probe; blocked, gated, SSRF, unscoped, unknown-behaviour and no-assertion each rejected in their own test |
| findings require evidence | PASS | 4-rung ladder, `INSUFFICIENT_EVIDENCE` as the floor, 5 distinct evidence gaps checked |
| duplicate findings are controlled | PASS | partial unique index on Phase 2's fingerprint |
| project isolation passes | PASS | cross-project probe refused; unscoped probe refused |
| risk/history guides exploration where evidence exists | PASS | Phase 4's prioritisation read once per session; absence reported as absence |
| tests pass | PASS | 29/29 |
| regression passes | PASS | 1909 / 0 failed / 4 ignored |
| documentation complete | PASS | report |

---

## The audit finding

**A bounded agent runtime already existed, was already in production use, and had simply never
been pointed at exploration.** `agent.rs` supplies `run_agent_loop`, `AgentLimits`,
`AgentContext` and cancellation; `agent_tools.rs` supplies the tool contract and safety levels;
`governed_healing::auto_diagnose` already drives a real agent through it in a live request; and
two production agents (`SelfHealingAgent`, `TestPlanningAgent`) implement the trait.

So Phase 5 added **a third agent on the existing runtime**, following the same stateless shape
as `SelfHealingAgent`: what it knows comes from `context.history`, not from private state.

`agents/exploration.rs` contains **no execution code at all**. That is what makes "no second
browser executor" checkable rather than aspirational — the only place a probe reaches an
executor is `exploration_execution.rs`, which is nothing but `insert_run_for_release` →
`ingest_run` → read the telemetry back.

---

## The central negative result

**Without a browser-backed executor that reports per-assertion outcomes, exploration can
observe the pipeline but cannot confirm a defect.** This is now a test, not a caveat:

* `execution_telemetry.assertion_result` is a **real column that no writer ever fills** — every
  telemetry row this system produces has it `NULL`;
* so "assertions were evaluated" can only be *declared* by the dispatcher;
* therefore `AssertionEvidence::NotEvaluated` is the default;
* therefore **every default-path finding is `INSUFFICIENT_EVIDENCE`** — asserted by
  `the_production_default_cannot_produce_a_confirmed_defect`, which feeds a runner whose every
  probe fails and still gets no confirmed defect.

The column is consulted *first*, so filling it is a one-sided change with no edit here.

---

## Evidence before verdict

`CONFIRMED_DEFECT` requires **all four** of: a recorded failing execution, a real canonical
fingerprint, a stated expectation, and a covering requirement. Omit any one and the class drops
— asserted one input at a time. `INSUFFICIENT_EVIDENCE` is reached by five distinct routes
(refused, unevaluated, assertions-held, no-fingerprint, default) and is the floor, not a
footnote. Every finding carries the `basis` clause that established it: the evidence, not the
verdict.

---

## Bounded twice, on purpose

Four exploration ceilings (actions, state transitions, findings, pages) are enforced in the
agent's `step` **and independently in `probe_action`**. The second is the one that matters: *an
agent that miscounts is a bug; a tool that acts anyway is a vulnerability.* A test pushes the
tool four times past a one-action budget and watches the counter stay at 1. Every stop is named
(`budget_exhausted: max_actions`, `catalogue_exhausted`, `no_surface`, `limits_reached`,
`cancelled`) and recorded verbatim on the session row.

---

## Duplicate suppression is structural

A partial unique index on `(project_id, failure_fingerprint)` — Phase 2's fingerprint, computed
from the *recorded* failure — makes "the same underlying defect" a property of the schema
rather than of application code remembering what it has seen. NULLs are **deliberately not**
folded: an inconclusive observation is not a claim about the application and must not stop the
same inconclusive observation being recorded later.

---

## Two gaps reported rather than faked

* **Back/forward navigation: `UNSUPPORTED`.** No `Action` variant exists; adding one would be a
  second Action AST.
* **Refresh: `APPROXIMATED`.** Re-navigates to the same URL, and every finding records the
  behaviour name so a reader knows which technique ran.

Both are returned by `behaviour_support_report()` and asserted by a test.

---

## Bugs found

1. **`BehaviourSupport` could not be serialized** — `#[serde(tag)]` cannot flatten a newtype
   variant's payload. Caught by a test, because the report is built with `json!`. A type that
   cannot serialize is a type that cannot be reported over HTTP.
2. **`FindingClass`'s derived order contradicted its meaning** — declared strongest-first, so
   `<` meant "weaker". Reordered so the derived `Ord` *is* evidence strength.
3. **Two wiring bugs I introduced**, caught by the compiler: a splice that consumed
   `impl Store`'s closing brace, and a regex edit that moved row mappers inside the `impl`. Both
   repaired by brace-depth verification rather than by eye.
4. **My own fixtures were in the wrong wire format** — `CrawlOutput` is `camelCase`, `Action`
   is `#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]`. Fixed *to* the real format
   rather than by relaxing the structs, so the tests now prove the real wire form round-trips.
5. **A foreign key caught six fixtures** — `exploration_findings.session_id` references
   `exploration_sessions`, so a finding cannot exist without its session. The constraint did its
   job; the fixtures were fixed to open one through the real writer.

Nothing was weakened, skipped or deleted to make a test pass.

---

## Performance — measured, not pinned

40 cases × 3 executions = 120 telemetry rows:

| | |
|---|---|
| memory / risk retrieval | **9 ms** |
| evidence collection (50 writes) | **31 ms** |
| duplicate detection (50 same-fingerprint inserts) | **7 ms** |
| a whole bounded session | **6 actions in 55 ms** |

Duplicate detection is the cheapest path because the unique index absorbs almost the entire
cost — which matters, since suppression runs on every finding. Risk retrieval is read once per
session, not once per probe. No timing threshold is pinned in CI; the measurement is
`#[ignore]`d so it can be reproduced rather than asserted on someone else's hardware.

---

## Limitations carried forward

1. **`assertion_result` is never populated** — the bound on what can be confirmed today.
2. **No browser executor in this environment** — assertion semantics live in the Playwright
   executor's TypeScript process.
3. **Back/forward unsupported; refresh approximated** — the Action AST cannot express them.
4. **Risk cannot steer to a route** — Phase 4 ranks test cases and there is no `covers` edge.
5. **A non-composable behaviour consumes a turn** — honest but wasteful; bounded, so safe.
6. **No requirement linkage yet** — `violated_requirement` is caller-supplied, so most findings
   cap at `POTENTIAL_DEFECT`/`OBSERVED_ANOMALY`. Wiring the existing `requirement_lookup` tool
   needs no schema change.
7. **The surface is only as good as the last crawl.**
8. **The live database is unchanged** — no figure here is a production claim.
9. **No E2E CI job** — pre-existing.

---

## Discipline

* No test weakened, skipped or deleted. Zero regressions.
* No second agent runtime, browser executor, Action AST or evidence system.
* Two wiring bugs I introduced were caught by the compiler; two design errors were caught by
  tests written before them.
* The most valuable output of this phase is a negative result, and it is enforced by a test
  rather than stated in prose.
* The three pre-existing migration guards fired and were moved with a comment naming why the
  new migration is additive — not silenced.

---

**Phase 6 not started. Nothing committed. Nothing pushed.**
