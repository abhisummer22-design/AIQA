pub mod adaptive_intelligence;
pub mod agent;
pub mod agent_execution;
pub mod agent_tools;
pub mod agents;
pub mod ai_generation;
pub mod ai_router;
pub mod ai_usage;
pub mod api;
pub mod api_testing;
pub mod app_source;
pub mod auth;
pub mod autopilot;
pub mod autopilot_api;
pub mod autopilot_cases;
pub mod autopilot_estimate;
pub mod autopilot_execute;
pub mod autopilot_generate;
pub mod autopilot_pipeline;
pub mod autopilot_report;
pub mod autopilot_runner;
pub mod autopilot_steps;
pub mod autopilot_store;
pub mod ci;
pub mod coverage;
pub mod crawler;
pub mod custom_testing;
pub mod doc_extract;
pub mod domain;
pub mod engine;
pub mod errors;
pub mod evidence;
pub mod execution_plan;
pub mod executor;
pub mod gap_check;
pub mod gap_verify;
pub mod governed_healing;
pub mod grounding;
pub mod intelligence;
pub mod json_schema;
pub mod junit;
pub mod log_bus;
pub mod login_suggest;
pub mod metrics;
pub mod mobile;
pub mod openapi_api;
pub mod openapi_cases;
pub mod openapi_runs;
pub mod openapi_spec;
pub mod operations;
pub mod performance_testing;
pub mod plan_validator;
pub mod policy;
/// Phase 3 (QA Intelligence Memory): deterministic retrieval of historical QA
/// intelligence — module risk, test stability, release/requirement/bug history.
/// Reuses `execution_telemetry`, the quality graph and the Phase 2 entities; adds
/// no second memory store and no second flakiness engine.
pub mod qa_memory;
/// Phase 5 (QA Intelligence Memory): module and release risk, built on the
/// existing adaptive_intelligence::calibrate_risk rather than a second score.
pub mod qa_memory_risk;
pub mod regression;
pub mod regression_api;
pub mod regression_runner;
pub mod requirement_generation;
pub mod requirements;
pub mod scheduler;
pub mod skill_files;
pub mod state;
pub mod store;
pub mod tools;
/// Phase 1 (Trace Graph Completion): the single trace walk,
/// Requirement -> Test -> Execution -> Failure -> Bug -> Fix -> Retest -> Release.
/// Adds no table and no subsystem - it reads the graph that already exists.
pub mod trace;
pub mod webhook;

#[cfg(test)]
mod adaptive_intelligence_tests;
#[cfg(test)]
mod agent_execution_tests;
#[cfg(test)]
mod agent_tests;
#[cfg(test)]
mod api_tests;
#[cfg(test)]
mod autopilot_test_support;
/// Phase 3 (Change Impact Intelligence + Smart Regression): resolves a declared change
/// against the stored graph and selects a regression set from the existing scope. No new
/// engine and no new graph vocabulary - see the module doc for why the empty
/// `change_impacts` table was never simply "missing an INSERT".
pub mod change_impact;
/// Phase 3 tests: impact resolution, the selection tiers, persistence and idempotence,
/// project isolation, the safety-floor retention guarantee, and the extension added to the
/// existing regression scope. Every row is produced by a real production writer.
#[cfg(test)]
mod change_impact_tests;
#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod governed_healing_tests;
/// Phase 6 (Data Integrity + Cross-Layer Intelligence): a read-only verifier that
/// compares facts independently recorded by the UI, API, backend, database and audit
/// layers. Read-only by construction: it holds a `&Store` and calls existing readers only.
pub mod integrity;
#[cfg(test)]
mod key_scope_tests;
#[cfg(test)]
mod operations_tests;
#[cfg(test)]
mod process_lifetime_maps_tests;
/// Phase 2: the "existing database" half of the migration test list — a database
/// that already holds rows must gain the new columns and keep all of them. Skipped
/// unless `AIQA_PRE_EXISTING_DB` names a *copy* of a pre-0028 file.
#[cfg(test)]
mod qa_memory_migration_tests;
/// Phase 3 (QA Intelligence Memory): retrieval — module risk, test stability,
/// release/requirement/bug history. One test per line of the master plan's Phase 3
/// test list, plus the §3 retrieval rules asserted directly.
#[cfg(test)]
mod qa_memory_retrieval_tests;
/// Phase 2 (QA Intelligence Memory): the persistent foundation — the durable
/// test-case <-> execution link, releases, and modules on the existing quality
/// graph. See docs/qa-intelligence-memory/PHASE_1_AUDIT.md.
#[cfg(test)]
mod qa_memory_tests;
/// Phase 2 (End-to-End QA Traceability): the full
/// Requirement -> Test -> Execution -> Failure -> Bug -> Fix -> Retest -> Release chain,
/// driven entirely through production writers - no hand-written fingerprint is used to
/// make a join succeed, because that is the defect this phase fixes.
#[cfg(test)]
mod trace_chain_tests;
/// Phase 1 (Trace Graph Completion): relation persistence, stable-id identity, the
/// trace walk's honesty rules, project isolation, idempotence, and the "no new
/// migration" claim - all asserted against a real migrated database rather than a
/// fixture-shaped mock.
#[cfg(test)]
mod trace_tests;

/// Phase 6 (Data Integrity + Cross-Layer Intelligence) tests. Over half negative: the
/// failure modes that matter for a verifier are all over-statements.
#[cfg(test)]
mod integrity_tests;

/// Risk Signal Correctness Review / Remediation — a later finding against Phase 4, not part of
/// it. Proves that a measured zero is distinguishable from an unmeasured signal, that an
/// unavailable signal no longer becomes 0.0, and that the score is no longer understated.
#[cfg(test)]
mod risk_signal_tests;

/// Phase 5 (AI Exploratory Testing Agent): bounded exploration over the existing agent
/// runtime, action AST, plan validator and policy gate. Over half the suite is negative -
/// every failure mode that matters for an autonomous agent is an over-statement.
#[cfg(test)]
mod exploration_tests;
/// Phase 6 (QA Intelligence Memory): performance and storage **measurement** — index
/// usage via `EXPLAIN QUERY PLAN`, retrieval latency on a large history, signal
/// bounding, and the storage cost of one remembered execution.
#[cfg(test)]
mod qa_memory_perf_tests;
/// Phase 4 (QA Intelligence Memory): memory → test-planning intelligence. One
/// test per line of the master plan's Phase 4 test list, plus the §6
/// explainability rules and the window rule that keeps old history from dominating.
#[cfg(test)]
mod qa_memory_planning_tests;
/// Phase 5 (QA Intelligence Memory): module and release risk. One test per line of
/// the master plan's Phase 5 test list, plus the invariant that a 0–1 risk score is
/// never emitted from partial evidence.
#[cfg(test)]
mod qa_memory_risk_tests;
/// Phase 6 (QA Intelligence Memory): full-flow validation end to end, plus the
/// data-integrity and security checks that are properties of the chain rather than
/// of any one step.
#[cfg(test)]
mod qa_memory_validation_tests;
/// RBAC testing that measures the server instead of describing it: `run_probe`
/// sends a real request with a real key and reports the real answer. See
/// `rbac_probe`'s doc comment for why this is not a declared matrix.
pub mod rbac_probe;
#[cfg(test)]
mod retention_and_metrics_tests;
/// Phase 4 (AI Risk-Based Testing + Test Prioritization): per-test risk over the existing
/// `calibrate_risk` model, and the prioritisation order built on it. Half the suite is
/// negative - the failure modes for risk are all over-statements.
#[cfg(test)]
mod risk_prioritisation_tests;

#[cfg(test)]
mod rbac_probe_tests;

#[cfg(test)]
mod rbac_matrix_tests;
#[cfg(test)]
mod run_map_retention_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod ws_tests;
