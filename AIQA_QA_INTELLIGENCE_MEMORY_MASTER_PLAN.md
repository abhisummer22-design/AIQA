# AIQA — QA Intelligence Memory
# Master 6-Phase Implementation & Validation Plan

## Document Purpose

This is the master execution plan for implementing the **QA Intelligence Memory** capability in the AIQA project.

The implementation will be performed **phase by phase**.

The agent MUST NOT implement all six phases blindly in one pass.

For every phase, the mandatory lifecycle is:

> INSPECT → PLAN → IMPLEMENT → TEST → FIND BUGS → FIX BUGS → RETEST → REGRESSION → DOCUMENT → PASS GATE → NEXT PHASE

A phase is considered complete only when its PASS gate is satisfied.

---

# 1. GLOBAL RULES

## 1.1 Current Repository Is the Source of Truth

Before changing code:

1. Inspect the current repository.
2. Inspect existing architecture.
3. Inspect migrations/schema.
4. Inspect persistence/store/repository code.
5. Inspect existing IDs and relationships.
6. Inspect existing tests.
7. Reuse existing functionality wherever possible.

Do NOT assume that a capability is missing merely because it is not obvious from documentation.

Do NOT create duplicate architecture when an existing implementation can be extended.

---

## 1.2 Do Not Rewrite Existing Architecture

The QA Intelligence Memory implementation MUST integrate with existing AIQA capabilities, especially:

- Adaptive Intelligence
- Failure Intelligence
- Regression
- Coverage
- Requirement Intelligence
- Execution
- Evidence
- Quality Graph
- Flakiness intelligence
- Healing intelligence
- AI usage tracking
- Existing persistence/store layer
- Existing authentication/project/tenant scoping

Do not replace these systems.

Extend them only where necessary.

---

## 1.3 No Duplicate Memory Systems

Before creating any new table, model, service, repository, API, or event:

1. Search the repository.
2. Determine whether equivalent functionality already exists.
3. Reuse existing entity IDs and persistence where possible.
4. Extend existing structures when appropriate.
5. Document why a new structure is required if one is unavoidable.

The goal is ONE coherent QA Intelligence Memory architecture, not multiple competing memory systems.

---

## 1.4 Historical Claims Must Be Evidence-Based

The system must never claim historical QA knowledge without stored evidence.

Examples:

Bad:

> "This module usually fails."

Good:

> "This module had 7 failed executions across the last 20 recorded runs."

Historical intelligence must be derived from actual persisted records.

Never invent:

- Previous bugs
- Previous failures
- Previous test results
- Previous releases
- Previous risk scores
- Previous healing outcomes
- Previous coverage
- Previous execution history

---

## 1.5 No Chain-of-Thought Storage

QA Intelligence Memory should store structured QA facts, telemetry, outcomes, relationships, metrics, and evidence references.

Do NOT store hidden reasoning or chain-of-thought.

Store concise structured explanations where required, such as:

- reason
- evidence reference
- failure category
- confidence
- source
- timestamp

---

# 2. MANDATORY PHASE EXECUTION PROTOCOL

For EVERY phase, follow exactly this process.

## STEP A — Inspect

Before coding:

- Read relevant source files.
- Read related migrations.
- Read existing tests.
- Search for duplicate functionality.
- Identify dependencies.
- Identify affected APIs.
- Identify backward-compatibility risks.

Produce an internal or documented implementation plan before modifying code.

---

## STEP B — Implement

Implement ONLY the current phase.

Do not start future-phase functionality unless it is an unavoidable prerequisite.

Keep changes focused and incremental.

---

## STEP C — Test

After implementation:

1. Run unit tests.
2. Run integration tests relevant to the phase.
3. Run existing regression tests relevant to affected functionality.
4. Run static checks/lint/type checks where applicable.
5. Run migration/schema validation where applicable.

Do not declare success based only on compilation.

---

## STEP D — Bug Discovery

If tests fail:

Create a defect entry containing:

- Bug ID
- Phase
- Component
- Reproduction
- Expected result
- Actual result
- Root cause
- Fix
- Test proving the fix

---

## STEP E — Fix

Fix all bugs introduced by or exposed by the current phase before proceeding.

Do not simply disable failing tests.

Do not weaken assertions just to make tests pass.

Do not remove existing coverage to hide failures.

---

## STEP F — Retest

After every fix:

- Re-run the failed test.
- Re-run related tests.
- Re-run the phase test suite.
- Re-run relevant regression tests.

---

## STEP G — Regression

Confirm that existing AIQA functionality still works.

Pay special attention to:

- Existing persistence
- Existing execution
- Existing regression
- Existing adaptive intelligence
- Existing coverage
- Existing failure intelligence
- Existing authentication/project isolation
- Existing APIs

---

## STEP H — Phase Report

Create/update:

`docs/qa-intelligence-memory/PHASE_<N>_REPORT.md`

The report MUST contain:

- Objective
- Existing functionality reused
- Files changed
- Database/schema changes
- APIs changed/added
- Tests added
- Tests executed
- Tests passed
- Tests failed
- Bugs found
- Bugs fixed
- Regression result
- Known limitations
- Architecture impact
- Phase status

---

## STEP I — PASS GATE

A phase can be marked PASS only if:

- Implementation is complete.
- Tests pass.
- Bugs introduced by the phase are fixed.
- Relevant regression passes.
- No known critical/blocking defect remains.
- No duplicate architecture was introduced.
- Persistence/data integrity is verified.
- Project/tenant isolation is preserved where applicable.
- Documentation/report is updated.

If the phase does NOT PASS:

> STOP.

Fix the phase.

Do NOT continue to the next phase.

---

# 3. PHASE 1 — QA INTELLIGENCE MEMORY AUDIT & DATA MAPPING

## Objective

Understand what historical QA intelligence already exists and what is missing.

This phase is primarily an architecture/data audit.

## Inspect

Map the following chain:

> Execution → Test Case → Failure → Bug → Requirement → Module → Environment → Build/Release

Also inspect:

- Existing database migrations
- Existing stores/repositories
- Existing execution records
- Existing failure fingerprints
- Existing requirement records
- Existing test case records
- Existing coverage records
- Existing regression history
- Existing environment/build information
- Existing healing history
- Existing quality graph
- Existing project/tenant boundaries
- Existing IDs and foreign keys

## Determine

For each entity identify:

- Exists in database?
- Exists only in runtime?
- Stable ID?
- Historical record?
- Relationship to other entities?
- Can be reused?
- Missing fields?
- Missing relationships?
- Missing timestamps/versioning?
- Missing project/tenant scope?

## Deliverable

Create:

`docs/qa-intelligence-memory/PHASE_1_AUDIT.md`

Include:

1. Current architecture map
2. Existing persistence map
3. Existing historical data
4. Missing historical data
5. Existing IDs
6. Missing relationships
7. Duplication risks
8. Recommended minimal architecture
9. Migration requirements
10. Phase 2 implementation plan

## Phase 1 Rule

Do not perform a large implementation before understanding the existing persistence model.

## PASS GATE

Phase 1 PASS requires:

- Architecture mapped
- Persistence mapped
- Existing memory capabilities identified
- Missing capabilities identified
- Stable IDs identified
- Relationships identified
- Minimal implementation design documented

Then proceed to Phase 2.

---

# 4. PHASE 2 — HISTORICAL QA MEMORY FOUNDATION

## Objective

Create the persistent foundation required to retain useful historical QA intelligence.

## Memory Should Connect

At minimum:

> Project
> → Release/Build
> → Environment
> → Requirement
> → Module
> → Test Case
> → Execution
> → Result
> → Failure
> → Bug
> → Evidence

Reuse existing entities whenever possible.

## Requirements

Support historical tracking for:

- Test execution outcomes
- Test stability
- Failure recurrence
- Requirement coverage history
- Module history
- Build/release history
- Environment association
- Bug recurrence
- Evidence references
- Existing failure fingerprints
- Healing outcomes where already supported

## Data Quality

Ensure:

- Stable IDs
- Timestamps
- Project/tenant scope
- Build/release association where available
- Environment association where available
- Referential integrity
- No duplicate historical records
- Safe migrations
- Backward compatibility

## Important

Do not create a second failure intelligence system.

Integrate with existing failure/fingerprint structures.

## Testing

Test:

- Fresh database
- Existing database
- Migration
- Insert
- Read
- Update where allowed
- Historical append behavior
- Duplicate prevention
- Project isolation
- Invalid references
- Partial/missing metadata

## PASS GATE

Phase 2 PASS only after:

- Memory foundation works
- Migrations pass
- Existing functionality still works
- Historical records persist correctly
- Tests pass
- Regression passes

---

# 5. PHASE 3 — QA INTELLIGENCE RETRIEVAL

## Objective

Turn stored historical records into useful, deterministic QA intelligence.

## Required Intelligence

Implement retrieval capabilities for:

### Module Risk History

Examples:

- failure rate
- recent failures
- recurring failures
- affected tests
- affected releases

### Test Stability

Examples:

- pass/fail history
- flaky behavior
- failure frequency
- recent instability

### Failure History

Examples:

- fingerprint recurrence
- failure category
- affected tests
- affected modules
- affected releases
- last occurrence

### Requirement History

Examples:

- historical coverage
- test execution history
- previous failures
- gaps

### Bug History

Examples:

- recurring defects
- affected module
- related tests
- recurrence frequency
- last seen

### Release History

Examples:

- execution summary
- failure trends
- regression results
- risk indicators

## Retrieval Rules

Results must be:

- Project scoped
- Deterministic
- Evidence backed
- Time-aware
- Explainable
- Bounded in query size

Do not return unsupported conclusions.

## Testing

Test:

- Empty history
- One record
- Large history
- Repeated failure
- Multiple projects
- Multiple environments
- Multiple releases
- Time-window queries
- Pagination/limits where applicable
- Missing metadata

## PASS GATE

All retrieval APIs/components work and pass regression.

---

# 6. PHASE 4 — MEMORY → TEST PLANNING INTELLIGENCE

## Objective

Make AIQA actually USE historical memory when planning tests.

The system should move from:

> "Generate generic tests."

to:

> "Generate tests informed by what historically failed, changed, remained unstable, or was risky."

## Planning Signals

Use historical memory for:

- High-risk modules
- Repeated failures
- Recurring bugs
- Unstable tests
- Historical requirement gaps
- Previous release failures
- Regression history
- Failure categories
- Reliable/ineffective test strategies where already available

## Example

If payment historically has:

- repeated timeout failures
- recurring validation bugs
- unstable API tests

Then the next test plan should increase relevant coverage.

The system must not invent these signals.

## Integration Rules

Reuse:

- Existing TestPlanningAgent
- Existing coverage engine
- Existing adaptive intelligence
- Existing failure intelligence
- Existing strategy reliability

Do not create a second planning engine.

## Explainability

Planning decisions should have structured reasons such as:

- Historical signal
- Metric
- Evidence reference
- Confidence
- Time window

Do NOT expose or store chain-of-thought.

## Testing

Verify:

- Memory influences planning when evidence exists
- Memory does not influence planning when no evidence exists
- Old irrelevant history does not dominate current signals
- Project isolation
- Requirement-specific planning
- Risk prioritization
- Regression behavior

## PASS GATE

Planning uses historical intelligence correctly without breaking existing planning behavior.

---

# 7. PHASE 5 — RELEASE & RISK INTELLIGENCE

## Objective

Use historical QA memory to provide release-level and module-level risk intelligence.

## Capabilities

Build or extend:

### Module Risk

Possible inputs:

- Recent failures
- Failure recurrence
- Bug recurrence
- Test instability
- Requirement gaps
- Recent changes
- Release history

### Release Risk

Possible inputs:

- Regression results
- High-risk modules
- Open/recurring defects
- Failure trends
- Coverage gaps
- Test stability
- Environment signals

### Historical Comparison

Compare:

> Current Release vs Previous Releases

Where data exists.

## Output

Provide structured risk information such as:

- Risk level
- Risk score if already supported
- Main contributing signals
- Evidence
- Affected modules
- Affected requirements
- Trend

Never fabricate a score or historical comparison.

If insufficient data exists:

> INSUFFICIENT_DATA

is preferable to a fabricated risk conclusion.

## Testing

Test:

- No historical data
- Small historical dataset
- Large dataset
- Improving trend
- Worsening trend
- Repeated failures
- Repeated bugs
- High-risk module
- Cross-project isolation
- Release comparison

## PASS GATE

Release/module risk intelligence is evidence-based and regression-safe.

---

# 8. PHASE 6 — FULL VALIDATION & PRODUCTION HARDENING

## Objective

Validate the complete QA Intelligence Memory system end-to-end.

## Full Flow

Validate:

> Requirements
> → Requirement Intelligence
> → Test Planning
> → Test Execution
> → Evidence
> → Failure Intelligence
> → Historical Memory
> → Retrieval
> → Risk Intelligence
> → Future Test Planning

## Full Regression

Run:

- Unit tests
- Integration tests
- API tests
- Database tests
- Migration tests
- Existing AIQA regression tests
- Relevant end-to-end tests
- Static checks
- Security checks
- Project isolation checks

## Security

Verify:

- Tenant/project isolation
- Authorization
- No cross-project historical leakage
- Sensitive data is not stored unnecessarily
- Credentials/secrets are not stored in memory
- No chain-of-thought storage
- Historical APIs respect authorization

## Performance

Check:

- Query performance
- Large historical datasets
- Index usage where applicable
- Memory retrieval latency
- Planning overhead
- Storage growth

Do not prematurely optimize without evidence.

## Data Integrity

Verify:

- No orphaned records
- No duplicate history
- Correct timestamps
- Correct relationships
- Correct project scope
- Safe migrations

## Final Deliverables

Create:

`docs/qa-intelligence-memory/FINAL_VALIDATION_REPORT.md`

Include:

- Complete architecture
- All phases
- Files changed
- Database changes
- APIs
- Tests
- Regression results
- Bugs found/fixed
- Performance observations
- Security observations
- Known limitations
- Future improvements
- Final PASS/FAIL

---

# 9. FINAL ACCEPTANCE CRITERIA

QA Intelligence Memory is considered COMPLETE only when all are true:

- [ ] Phase 1 PASS
- [ ] Phase 2 PASS
- [ ] Phase 3 PASS
- [ ] Phase 4 PASS
- [ ] Phase 5 PASS
- [ ] Phase 6 PASS
- [ ] Historical QA data persists correctly
- [ ] Historical retrieval works
- [ ] Failure history works
- [ ] Test stability history works
- [ ] Requirement history works
- [ ] Bug recurrence works where data exists
- [ ] Release history works
- [ ] Test planning uses historical evidence
- [ ] Risk intelligence uses historical evidence
- [ ] No fabricated historical claims
- [ ] No chain-of-thought storage
- [ ] No duplicate intelligence architecture
- [ ] Project/tenant isolation verified
- [ ] Existing AIQA functionality remains stable
- [ ] Full regression passes

---

# 10. ABSOLUTE STOP CONDITIONS

The agent MUST STOP and ask for human review if:

1. A migration could destroy existing data.
2. Existing architecture conflicts with the proposed design.
3. A required entity cannot be reliably linked.
4. A change could break project/tenant isolation.
5. Existing tests contradict the proposed behavior.
6. The agent discovers a major undocumented architectural dependency.
7. A phase cannot reach PASS without weakening tests.
8. A change requires rewriting a major existing subsystem.
9. Historical data cannot support a requested intelligence claim.
10. The agent is uncertain whether existing functionality should be replaced or extended.

Never silently make a destructive architectural decision.

---

# 11. PHASE HANDOFF PROTOCOL

At the end of every successful phase:

Create:

`docs/qa-intelligence-memory/PHASE_<N>_PASS.md`

It must contain:

- Phase number
- Completion date
- Summary
- Files changed
- Tests run
- Tests passed
- Bugs found
- Bugs fixed
- Regression result
- Known limitations
- Next phase prerequisites

The next phase MUST read:

1. Previous phase report
2. Previous phase PASS file
3. Relevant source code
4. Relevant tests

before making changes.

---

# 12. MASTER EXECUTION COMMAND

When this document is given to the coding agent, follow this rule:

> Execute the six phases sequentially. Never skip a phase. Never start the next phase until the current phase reaches PASS. After implementation, test the current phase, identify failures, fix them, retest, run relevant regression tests, document the results, and only then continue. Preserve the existing AIQA architecture and reuse existing systems. Do not create duplicate intelligence or memory subsystems. If a destructive, ambiguous, or architecture-breaking decision is required, STOP and request human review.

---

# 13. IMPORTANT FOR FREE/LOW-CONTEXT MODELS

Because implementation may be performed using a free or limited-context coding model:

- Keep each phase focused.
- Do not attempt to modify the entire repository at once.
- Read files before editing.
- Make small, reviewable changes.
- Run tests frequently.
- Save decisions in markdown reports.
- Never rely on conversation memory for previous phase decisions.
- Read previous phase reports from the repository.
- Do not claim completion without test evidence.
- Do not move forward after a failed phase gate.

The repository documentation and phase reports are the persistent project memory.

---

# END STATE

The desired final architecture is:

Requirements
    ↓
Requirement Intelligence
    ↓
Risk / Historical Signals
    ↓
Test Planning
    ↓
Coverage Planning
    ↓
Execution
    ↓
Evidence
    ↓
Failure Intelligence
    ↓
QA Intelligence Memory
    ↓
Historical Retrieval
    ↓
Risk / Release Intelligence
    ↓
Future Test Planning
    ↓
Continuous QA Learning

The goal is not simply to "store old test results."

The goal is to make AIQA progressively smarter using **real, traceable, project-scoped historical QA evidence**.
