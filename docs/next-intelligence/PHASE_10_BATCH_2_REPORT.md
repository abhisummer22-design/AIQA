# AIQA Phase 10 — Batch 2: Project-Scoped Evidence Retrieval Engine Report

**Date:** 2026-10-11  
**Status:** COMPLETE (PASS)  
**Authoritative Reference:** `AIQA_NEXT_ROADMAP_PHASED_PLAN.md` §"PHASE 10 — AI QA COPILOT" (L369–431)  
**Implementation Plan Reference:** `docs/next-intelligence/PHASE_10_QA_COPILOT_PLAN.md` §"Batch 2: Project-Scoped Evidence Retrieval Engine"  
**Predecessor Gate:** Phase 10 Batch 1 (PASS)

---

## 1. Executive Summary & Verification Verdict

Phase 10 Batch 2 implements the project-scoped evidence retrieval coordinator (`CopilotEvidenceCoordinator`) for the AI QA Copilot. The coordinator executes typed, bounded, project-scoped reads over existing engines based strictly on the parsed `CopilotIntent` and `CopilotDispatchPlan` from Batch 1.

### Verdict: **PASS**

- **Zero Memory Database:** No parallel database, no vector store, no cache tables introduced.
- **Zero Schema Migrations:** Strictly read-only queries over existing tables (0 new tables, 0 column changes).
- **Zero Dynamic / Arbitrary SQL:** 100% of queries use existing parameterized readers (`?` binds).
- **Zero External AI / Model Calls:** No LLM invocation, no answer synthesis, no prompt construction (strictly reserved for Batch 3).
- **Strict Multi-Tenant Scoping:** Every reader boundary filters explicitly by `project_id`; cross-project and cross-tenant evidence is never exposed.
- **Bounded Result Sizes:** Hard ceilings enforced across all retrieved collections (`MAX_FAILURES_RETRIEVED = 50`, `MAX_TESTS_RETRIEVED = 100`, `MAX_REQUIREMENTS_RETRIEVED = 100`, `MAX_IMPACTS_RETRIEVED = 50`, `MAX_QUALITY_CASES_RETRIEVED = 50`).
- **Honest Insufficient Evidence:** Missing or unmeasured evidence produces structured `InsufficientDataInfo` rather than fabricated values or disguised empty successes.
- **Error Propagation:** Reader/database failures propagate as `Err` rather than being masked as empty results.
- **Resource Discipline:** All owned database pools are awaited on close (`db.close().await`), leaving zero leaked SQLite lock files or directory handles.

---

## 2. Exact Files Created and Modified

### Created Files
1. `services/orchestrator/src/copilot_retrieval.rs`
   - Defines `CopilotEvidenceCoordinator`, `RetrievedEvidence`, `RetrievedEvidenceData`, `RetrievalStatus`, `InsufficientDataInfo`, `EvidenceProvenance`, and intent-specific evidence payloads.
   - SHA-256: `EDCD73BDA5BEC002A12D116712AFC18F5642101B8DE3F3E6367341368F421DC3`
2. `services/orchestrator/src/copilot_retrieval_tests.rs`
   - 10 comprehensive tests verifying reader sets, tenant isolation, bounds, failure propagation, skip semantics, injection immunity, and resource cleanup.
   - SHA-256: `5398067F916630AEF8A087C5F89E194FE417992CFF0215A66C34C70F6B292892`
3. `docs/next-intelligence/PHASE_10_BATCH_2_REPORT.md` (this report)

### Modified Tracked Files
1. `services/orchestrator/src/lib.rs`
   - Registered `pub mod copilot_retrieval;` and `#[cfg(test)] mod copilot_retrieval_tests;`.
   - Pre-modification SHA-256: `48D49610B4B491EBAF50135ECA359B36DE7F09B61473C602A14D9A2CB72A7676`
   - Post-modification SHA-256: `938DEE6C0DDE012E0F6B8F57D2FA7B7906FEC71B7D789F7DF0A893F1925E0653`

---

## 3. Intent-to-Reader Mapping

The coordinator executes only the readers declared by the dispatch plan for each intent:

| Intent Variant | Canonical Question | Required Readers Executed | Existing Source Methods Reused |
|---|---|---|---|
| `ReleaseFailureReason { release_id }` | *"Why did this release fail?"* | `ReleaseReadiness` | `Store::list_releases`, `release_readiness::assess_release` |
| `NextTestRecommendation` | *"What should I test next?"* | `ChangeImpact`, `QaMemoryRisk` | `QaMemoryReader::prioritise_tests`, `ChangeImpactReader::safety_floor`, `ChangeImpactReader::risk_scores` |
| `RegressionImpact { change_ref }` | *"Which changes affect my regression suite?"* | `ChangeImpact` | `Store::list_change_impacts`, `ChangeImpactReader::resolve`, `ChangeImpactReader::impacted_from_node`, `ChangeImpactReader::safety_floor` |
| `TestRiskExplanation { test_case_id }` | *"Why is this test high risk?"* | `QaMemoryRisk` | `Store::get_test_case`, `QaMemoryReader::test_risk`, `test_quality::assess_case`, `ChangeImpactReader::risk_scores` |
| `RecurringFailures` | *"Which failures are recurring?"* | `FailureClusters` | `Store::list_failure_clusters` |
| `RequirementCoverageGaps` | *"Which requirements lack test coverage?"* | `RequirementsCoverage` | `Store::list_requirements`, `Store::list_test_cases_for_requirement` |
| `InsufficientEvidenceReason { finding_id, run_id }` | *"Why was this finding classified as insufficient evidence?"* | `IntegrityVerifier`, `TestQuality` | `Store::list_run_ids`, `IntegrityVerifier::verify_run`, `Store::list_test_cases`, `test_quality::assess_case` |
| `ReleaseReadinessBlockers { release_id }` | *"What is blocking release readiness?"* | `ReleaseReadiness` | `Store::list_releases`, `release_readiness::assess_release` |

---

## 4. Coordinator Design and Reuse of Existing Interfaces

`CopilotEvidenceCoordinator` delegates all data access to existing production reader components without creating parallel query abstractions:

```rust
pub struct CopilotEvidenceCoordinator {
    store: Store,
    executor_kinds: Vec<String>,
}
```

- **Direct Interface Reuse:**
  - `release_readiness::assess_release(&self.store, project_id, release_id, &inputs)`
  - `ChangeImpactReader::new(self.store.clone())`
  - `QaMemoryReader::new(self.store.clone())`
  - `IntegrityVerifier::new(&self.store)`
  - `test_quality::assess_case(&self.store, &case, project_id, &inputs)`
  - `Store::list_failure_clusters`, `Store::list_requirements`, `Store::list_test_cases_for_requirement`, `Store::list_change_impacts`
- **Classification Result Routing:**
  - `retrieve_for_classification(&self, project_id, &classification)` inspects the `ClassificationResult`:
    - `Classified`: dispatches intent-scoped reader queries.
    - `ClarificationRequired`: sets `RetrievalStatus::Skipped`, record count = 0, zero reader invocations, zero DB queries.
    - `UnsupportedQuery`: sets `RetrievalStatus::Skipped`, record count = 0, zero reader invocations, zero DB queries.

---

## 5. Multi-Tenant Isolation & Security Enforcement

1. **Parameter Level Scoping:** Every single query executed by the coordinator binds `project_id` into the SQL statement (`WHERE project_id = ?`).
2. **Pre-Query Entity Scoping:** When looking up specific entities (e.g. `test_case_id` or `release_id`):
   - `Store::get_test_case(id)` returns `Option<TestCaseRow>`. The coordinator checks `case.project_id == project_id`. If it does not match, the case is treated as non-existent and refused.
   - `Store::list_releases(project_id)` checks if `release_id` is present in the project's release list. Another project's release is never answered from its data.
3. **Input Sanitization:** `validate_project_id` rejects project IDs containing SQL injection syntax (`'`, `"`, `;`, `/*`, `--`) or empty whitespace strings before any store interaction occurs.
4. **Untrusted Text Containment:** Parameterized SQL queries ensure that user-provided text in `change_ref` or entity IDs cannot inject SQL commands or break parameter boundaries.

---

## 6. Result Ceilings & Insufficient Evidence Representation

### Enforced Ceilings
- `MAX_FAILURES_RETRIEVED = 50`
- `MAX_TESTS_RETRIEVED = 100`
- `MAX_REQUIREMENTS_RETRIEVED = 100`
- `MAX_IMPACTS_RETRIEVED = 50`
- `MAX_QUALITY_CASES_RETRIEVED = 50`

When a query's underlying records exceed the ceiling, `is_truncated` is marked `true` and the record set is truncated to the bounded ceiling.

### Insufficient Evidence Representation
When evidence is missing, unmeasured, or absent:
- `RetrievalStatus::InsufficientData` is explicitly returned.
- Structured `InsufficientDataInfo` contains:
  - `reason`: Plain-language explanation of what was missing.
  - `missing_entity`: The identifier of the missing entity if applicable.
  - `suggested_action`: Actionable guidance to generate the required evidence.
- The coordinator never fabricates 0.0 scores or converts empty reads into passes.

---

## 7. Verification Results

### Targeted Batch 2 Tests (`copilot_retrieval_tests`)
Command: `cargo test copilot_retrieval_tests`  
Result: **10 passed; 0 failed; 0 ignored** in 0.23s.

1. `test_each_intent_requests_only_expected_reader_set` — **ok** (Pins 8 intents to only declared readers)
2. `test_valid_project_retrieves_only_its_own_evidence` — **ok** (Ensures project A retrieves only project A data)
3. `test_cross_project_and_tenant_evidence_never_returned` — **ok** (Project A cannot read project B release or test case)
4. `test_missing_evidence_produces_insufficient_data` — **ok** (Empty projects return structured `InsufficientData`)
5. `test_reader_failures_propagate_error_never_disguised_as_success` — **ok** (Pool closure propagates `Err`, never masked)
6. `test_bounded_limits_are_enforced` — **ok** (60 clusters truncated to 50 with `is_truncated = true`)
7. `test_unsupported_and_clarification_results_skip_retrieval` — **ok** (Clarification/unsupported results skip retrieval completely)
8. `test_sql_and_tenant_injection_cannot_alter_reader_or_expose_data` — **ok** (SQL injection rejected or sanitized)
9. `test_repeated_identical_inputs_are_deterministic` — **ok** (10 consecutive identical runs yield identical plans)
10. `test_resource_ownership_and_zero_test_artifact_leaks` — **ok** (Explicit `db.close().await` leaves zero leaked handles)

### Batch 1 Intent Classification Tests
Command: `cargo test copilot_intent_tests`  
Result: **9 passed; 0 failed; 0 ignored** in 0.01s.

### Phase 9 Regression Tests
Command: `cargo test phase_9_readiness_tests`  
Result: **21 passed; 0 failed; 0 ignored** in 0.89s.

### Phase 8 Regression Tests
Command: `cargo test phase_8_quality_tests`  
Result: **20 passed; 0 failed; 0 ignored** in 0.89s.

### Formatting Check
Command: `rustfmt --edition 2021 --check services/orchestrator/src/copilot_retrieval.rs services/orchestrator/src/copilot_retrieval_tests.rs services/orchestrator/src/copilot_intent.rs services/orchestrator/src/copilot_intent_tests.rs`  
Result: **Exit code 0 (0 formatting diffs)**.

### Clippy Check
Command: `cargo clippy --all-targets`  
Result: **Exit code 0 (0 new warnings; 0 errors)**. All 65 existing warnings are pre-existing outside Phase 10 code.

---

## 8. Resource Ownership & Artifact Leak Verification

- **Fixture Model:** `Db` encapsulates `Store` and `TempDir`.
- **Awaited Teardown:** `db.close().await` awaits `store.pool.close().await` before dropping `TempDir`.
- **Measurement:** `test_resource_ownership_and_zero_test_artifact_leaks` verifies that before fixture creation, count is 0; and after `db.close().await`, `tempdir.close()` succeeds without Windows file locking violations.

---

## 9. Repository Checkpoint

- **Working Directory:** `H:\AIQA`
- **Branch:** `main`
- **HEAD Commit:** `7930507785d203f3ff9cb3e575413bd95f04aabb`
- **Tracked Modifications:** 44 files (all pre-existing, preserved intact).
- **Staged Changes:** None (0 staged files).
- **Commits / Pushes:** None (0 commits created, 0 pushes attempted).

---

## 10. Next Steps

Batch 2 is complete and verified at **PASS**. In accordance with instructions:
- Batch 3 (Grounded Answer Synthesis, Fact/Inference Separation & Citation Engine) has **not** started.
- No model calls, LLM prompt generation, or API endpoints have been added.
- Awaiting human review before proceeding to Batch 3.
