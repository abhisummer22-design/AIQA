# AIQA — Next Roadmap: Phase-by-Phase Implementation Plan
## After QA Intelligence Memory, Traceability, Risk, Exploration, and Data Integrity

## Purpose

This plan defines the next steps to make AIQA more dependable as an AI QA Engineer / QA Command Center.

The previous reported work has completed:
- QA Intelligence Memory
- Trace Graph / QA Traceability
- Change Impact + Smart Regression
- Risk-Based Testing
- Exploratory Testing Agent
- Data Integrity + Cross-Layer comparison
- Risk Signal Remediation

However, several findings were left open and the latest repository has not yet been independently re-audited through the connected GitHub integration. Therefore, the next work starts with verification of the current repository state, not by assuming that the old reports necessarily match the latest branch.

## Mandatory execution lifecycle

Every phase must follow:

> INSPECT → PLAN → IMPLEMENT → TEST → FIND BUGS → FIX ROOT CAUSE → RETEST → REGRESSION → DOCUMENT → PASS GATE

A phase cannot advance until its PASS gate is genuinely met.

### Global rules

1. Read this plan and all relevant previous phase reports before any code changes.
2. Verify the current branch, HEAD, working tree, and recent commits.
3. Inspect existing implementation before deciding a feature is missing.
4. Extend existing systems; do not make duplicate memory, risk, bug, regression, planning, execution, or evidence systems.
5. Do not fabricate data, links, metrics, risk scores, historical claims, or test results.
6. Missing evidence must remain `UNMEASURED`, `UNKNOWN`, `INSUFFICIENT_DATA`, or equivalent.
7. Preserve project isolation, authorization, Action AST validation, executor policies, governed healing, and environment safety.
8. Never weaken, delete, or skip tests to get a green result.
9. If a test fails: determine root cause, fix it, rerun the failed test, and run relevant regression.
10. Do not commit or push unless the user explicitly authorizes it.
11. At the end of every phase, create the phase report and a PASS file only after real verification.
12. Stop for human review if the proposed change is destructive, architecture-breaking, security-sensitive, or requires a product decision.

Recommended report directory:
`docs/next-intelligence/`

---

# PHASE 6.1 — OPEN-FINDING AUDIT AND RISK CORRECTNESS

## Objective

Re-check the five previously reported open findings against the current source, tests, and latest branch. Do not assume they remain open or have been fixed.

Previously reported items:
- R1 — `GET /risk` may turn a read failure into a zero dependency-risk value.
- R2 — `change_frequency` is measured but may not feed any risk formula.
- R3 — no reliable source was found for `security_sensitivity` and `business_criticality`.
- F1 — `set_task_status` uses an `else if` pattern that may skip `finished_at` when both flags are true.
- L4 — two risk formulas may produce divergent results.

## Step 1: Establish repository truth

Record:
- remote URL
- current branch and HEAD
- `git status --short` and `git status -sb`
- latest commits
- current test counts
- whether the latest known commit is on the remote

Do not modify code during this initial audit.

## Step 2: Inspect each finding

For every item, classify it as:
- `OPEN_CONFIRMED`
- `ALREADY_FIXED`
- `NOT_REPRODUCIBLE`
- `NEEDS_PRODUCT_DECISION`
- `ACCEPTED_RISK`

Show source references, tests, and evidence.

### R1 — Read-failure behavior
Determine whether a failed read can be represented as measured zero. Decide whether the endpoint should:
- propagate the read error, or
- return an explicit partial/unavailable result.

Do not silently turn infrastructure/storage failure into a successful zero-risk value.

If behavior is a product/API compatibility decision, document the options and stop for the user's decision before implementing a breaking response change.

### R2 — `change_frequency`
Determine whether this is:
- a measured signal used by risk,
- a measured informational signal intentionally excluded from risk,
- or an accidentally unused signal.

If integrated, define its meaning, weighting, evidence requirements, and tests. Do not double-count the same change signal through multiple paths.

### R3 — Unsourced signals
Do not invent security sensitivity or business criticality. Keep them explicitly unmeasured unless an authoritative source exists. If adding a source is proposed, document the product/schema decision first.

### F1 — Task status timestamps
Inspect every caller of `set_task_status`. Determine whether the `else if` is currently reachable with both flags true. If fixing it, add a regression test that proves `started_at` and `finished_at` semantics, and check scheduler/run-state behavior.

### L4 — Risk formula divergence
Identify each formula, its inputs, weighting, normalization, thresholds, and API consumer. Decide whether one is intentionally a different metric or an accidental duplicate. Do not silently alter public scores without impact analysis.

## Deliverables

Create:
- `docs/next-intelligence/PHASE_6_1_OPEN_FINDINGS_AUDIT.md`

If code fixes are approved/appropriate, also create:
- `docs/next-intelligence/PHASE_6_1_REMEDIATION_REPORT.md`
- `docs/next-intelligence/PHASE_6_1_PASS.md`

## PASS GATE

PASS requires:
- all five items are individually classified with evidence;
- any fix has a regression test;
- no unsourced signal is invented;
- any API/product choice is either approved or explicitly left blocked;
- relevant tests and regression pass.

If a human decision is required, mark the phase `BLOCKED_ON_DECISION` rather than guessing.

---

# PHASE 6.2 — REAL-DATA END-TO-END INTEGRATION VALIDATION

## Objective

Verify that the previously added intelligence components work together through real application paths, not just unit tests or hand-constructed database states.

## Target chain

`Requirement → Test Case → Change → Impact → Prioritized Test → Execution → Evidence → Failure → Trace Graph → Risk / Integrity result`

Use a controlled test project and disposable test data. Do not claim that test fixtures prove production history.

## Validate

1. Requirement-to-test traceability.
2. Stable test identity through execution.
3. Execution telemetry and failure fingerprint consistency.
4. Failure-to-bug relation where a real bug record exists.
5. Change declaration and change-impact records.
6. Smart regression selection.
7. Risk prioritization and unavailable-signal handling.
8. Exploratory execution budget and action-policy enforcement.
9. Evidence persistence and duplicate-finding control.
10. Cross-layer comparisons only when entity identity and timestamps support them.
11. Project isolation across two independent projects.
12. Empty-history and insufficient-data handling.

## Required scenarios

- one completely passing journey;
- one controlled failure with recorded evidence;
- one supported change that affects related tests;
- one unrelated change that must not invent impacts;
- one missing trace link;
- one risk signal that is truly measured as zero;
- one risk signal that is unmeasured;
- one duplicate failure pattern across two projects, without cross-project data leakage;
- one cross-layer mismatch using the same known entity and aligned observation window;
- one stale or ambiguous observation that must not become a confirmed defect.

## Safety

- Use production writers where they exist.
- Avoid direct/manual SQL insertion to simulate a state the application cannot produce.
- Do not use production credentials or production data.
- Keep DB verification read-only.
- If a scenario cannot be generated through legitimate application paths, document the limitation instead of faking it.

## Deliverables

Create:
- `docs/next-intelligence/PHASE_6_2_INTEGRATION_VALIDATION_REPORT.md`
- `docs/next-intelligence/PHASE_6_2_PASS.md` only after genuine success

## PASS GATE

PASS requires:
- real persisted records are created through supported writers;
- end-to-end retrieval returns the correct chain;
- missing/ambiguous data remains missing/ambiguous;
- project isolation passes;
- relevant backend, frontend, type/lint, and regression checks pass;
- all discovered bugs are fixed and retested.

---

# PHASE 7 — PRODUCTION-LIKE USER JOURNEY INTELLIGENCE

## Objective

Test complete business journeys rather than only isolated cases, while reusing existing browser/API executors and Action AST.

## Example journey

`Login → Search → Product → Cart → Checkout → Payment → Order`

Actual steps must be derived from the user's configured application, requirements, crawl, and approved cases. Never assume every app has a commerce flow.

## Capabilities

1. Define a journey as ordered steps with prerequisites and state assertions.
2. Reuse existing test cases, crawls, role/login configuration, executors, scheduler, and evidence.
3. Verify state preservation between steps.
4. Attach requirement and trace-graph links where supported.
5. Identify the failing journey step and its evidence.
6. Support safe resume/replay only where idempotency and state are understood.
7. Report journey verdict separately from individual task verdicts.
8. Allow selecting journeys affected by a supported change-impact record.
9. Preserve role boundaries and environment restrictions.
10. Avoid duplicating an existing DAG or regression runner.

## Required tests

- complete successful journey;
- failure at an intermediate step;
- state not preserved between steps;
- invalid prerequisite order;
- duplicate step/action;
- retry/resume safety;
- role/authorization boundary;
- project isolation;
- unsupported or missing step evidence;
- cancellation and timeout;
- evidence and trace links;
- journey selection after a supported change.

## PASS GATE

A journey must be executable through existing governed paths, produce step-level evidence, report a coherent final verdict, and pass isolation and regression tests.

## Deliverables

- `docs/next-intelligence/PHASE_7_USER_JOURNEYS_REPORT.md`
- `docs/next-intelligence/PHASE_7_PASS.md`

---

# PHASE 8 — AI TEST CASE QUALITY INTELLIGENCE

## Objective

Measure whether generated and saved test cases are useful, traceable, unambiguous, non-duplicative, and executable.

## Proposed dimensions

Only score dimensions that can be measured honestly:
- requirement linkage/coverage;
- one clear condition per case;
- concrete, reproducible steps;
- observable expected result;
- negative/boundary coverage where the requirement supports it;
- duplicate similarity;
- automation feasibility against available executor capabilities;
- relevance to recorded risk/change signals.

## Scoring rules

1. Prefer deterministic rules for measurable checks.
2. Any AI-assisted judgment must be identified as a heuristic with evidence and confidence.
3. Explain every score and deduction.
4. Distinguish `SCORE_AVAILABLE` from `INSUFFICIENT_DATA`.
5. Do not treat a missing criticality value as low criticality.
6. Do not automatically delete, reject, or approve cases based on a score.
7. Avoid creating a second test-generation or validation engine.
8. Reuse the existing case validator, coverage techniques, trace links, memory, and risk signals.

## Required tests

- high-quality case;
- vague steps;
- expected result missing/untestable;
- duplicate and near-duplicate cases;
- requirement link missing;
- unavailable executor;
- insufficient evidence for a dimension;
- different projects with similar case text;
- scores reproducible for deterministic dimensions;
- AI heuristic result marked as such.

## PASS GATE

Quality scoring is explainable, reproducible where deterministic, honest about missing data, project-scoped, and does not silently change approval/execution behavior.

## Deliverables

- `docs/next-intelligence/PHASE_8_TEST_QUALITY_REPORT.md`
- `docs/next-intelligence/PHASE_8_PASS.md`

---

# PHASE 9 — RELEASE READINESS INTELLIGENCE

## Objective

Produce a release/build quality recommendation grounded in actual test and historical evidence.

## Signals, when available

- critical/high defects;
- failed regression;
- new failure clusters;
- risk-prioritized tests;
- requirement coverage gaps;
- flaky behavior;
- exploratory findings;
- data-integrity findings;
- change-impact scope;
- user journey outcomes;
- historical release comparisons.

## Output

A structured result should contain:
- `GO`, `CONDITIONAL_GO`, `NO_GO`, or `INSUFFICIENT_DATA`;
- contributing signals;
- blocked criteria;
- evidence references;
- affected requirements/modules where reliably linked;
- data freshness and missing-signal notes;
- rule/version used to make the decision.

## Rules

1. Do not use one arbitrary 0–100 score as the whole decision.
2. If a score is displayed, explain its formula and inputs.
3. Do not classify a release as safe solely because tests passed when required tests were not run.
4. Do not call a failure a product defect without sufficient evidence.
5. Preserve explicit business/user-defined release gates.
6. Use deterministic release-blocking rules for known critical conditions; use risk estimates only as supporting evidence.
7. Do not invent missing module links, criticality, historical releases, or risk signals.

## Required tests

- no historical data;
- all required checks pass;
- critical failure;
- high defect count;
- unexecuted mandatory test;
- insufficient requirement coverage;
- integrity mismatch;
- high flakiness;
- conflicting signals;
- stale evidence;
- different project isolation;
- same metrics with different missing-data conditions;
- historical comparison across real recorded releases.

## PASS GATE

Release recommendation must be explainable and project-scoped, recognize missing or stale evidence, follow explicit release gates, and pass regression tests.

## Deliverables

- `docs/next-intelligence/PHASE_9_RELEASE_READINESS_REPORT.md`
- `docs/next-intelligence/PHASE_9_PASS.md`

---

# PHASE 10 — AI QA COPILOT

## Objective

Provide a conversational interface over existing QA evidence and intelligence, without creating another memory database.

## Example questions

- Why did this release fail?
- What should I test next?
- Which changes affect my regression suite?
- Why is this test high risk?
- Which failures are recurring?
- Which requirements lack test coverage?
- Why was this finding classified as insufficient evidence?
- What is blocking release readiness?

## Architecture

The Copilot should:
1. Parse the question into a known query intent.
2. Retrieve evidence using existing project-scoped APIs/services.
3. Produce a concise answer based on returned records.
4. Cite entity IDs, run IDs, report links, timestamps, and evidence references where available.
5. Separate facts from inferences.
6. Return `INSUFFICIENT_DATA` when evidence is unavailable.
7. Ask for clarification when entity/project/time range is ambiguous.
8. Enforce the caller's existing role and project scope.

## Safety and privacy

- Do not store chain-of-thought.
- Do not expose secrets or credentials.
- Do not let natural-language questions generate arbitrary SQL or shell commands.
- Do not bypass authorization via tool calls.
- Do not hallucinate bugs, metrics, releases, or historical outcomes.
- Do not make the Copilot an autonomous mutation authority; use existing policies and human approvals.

## Required tests

- question answered from actual evidence;
- no evidence available;
- ambiguous project;
- unauthorized project;
- prompt injection contained in stored requirement/bug text;
- sensitive values redacted;
- contradictory sources;
- stale records;
- facts versus inference clearly separated;
- query limits and timeouts;
- no arbitrary SQL or shell execution.

## PASS GATE

Copilot answers must be grounded in retrieved records, project-scoped, clear about uncertainty, and unable to bypass execution/security governance.

## Deliverables

- `docs/next-intelligence/PHASE_10_QA_COPILOT_REPORT.md`
- `docs/next-intelligence/PHASE_10_PASS.md`

---

# GLOBAL VERIFICATION AFTER EACH PHASE

Run the checks that apply to changed code:
- formatting (`cargo fmt --check` or repository equivalent);
- clippy/lint with warnings policy;
- backend tests;
- frontend tests and type-check if frontend contracts change;
- migrations on a fresh database and an existing database if migrations change;
- focused integration/E2E tests;
- project-isolation tests;
- performance measurements for data-heavy paths.

Record actual commands, exit codes, counts, ignored tests, and known pre-existing flakes. Never claim a check ran if it did not.

At each phase end, report:
- PASS / BLOCKED / FAIL;
- files changed;
- schema/API changes;
- tests added/run/passed/failed/ignored;
- defects found/fixed;
- regression result;
- data/security/performance implications;
- known limitations;
- next phase readiness.

---

# RECOMMENDED EXECUTION ORDER

1. Phase 6.1 — Open-Finding Audit and Risk Correctness
2. Phase 6.2 — Real-Data End-to-End Integration Validation
3. Phase 7 — Production-like User Journey Intelligence
4. Phase 8 — AI Test Case Quality Intelligence
5. Phase 9 — Release Readiness Intelligence
6. Phase 10 — AI QA Copilot

Run **one phase per agent execution**. The agent may not begin the next phase automatically unless the user explicitly asks it to continue and the current phase has a genuine PASS.

## Immediate next action

Run only **Phase 6.1 audit-only** first. Do not implement fixes until the current state of R1, R2, R3, F1, and L4 is confirmed from the repository. Do not start Phase 7 until Phase 6.1 and Phase 6.2 are complete.

---

# FINAL TARGET

AIQA should evolve toward:

`Requirement`
→ `Traceability`
→ `Change Impact`
→ `Risk Prioritization`
→ `Quality Test Cases`
→ `Exploratory Testing`
→ `User Journeys`
→ `Cross-Layer Verification`
→ `Failure/Bug Intelligence`
→ `Release Readiness`
→ `Evidence-Grounded QA Copilot`

The purpose is not to add AI everywhere. The purpose is to make AIQA safer and more useful by combining real execution evidence, reliable relationships, explainable decisions, and strict governance.
