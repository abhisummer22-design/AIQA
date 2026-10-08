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
pub mod regression;
pub mod regression_api;
pub mod regression_runner;
pub mod requirement_generation;
pub mod requirements;
/// Phase 3 (QA Intelligence Memory): deterministic retrieval of historical QA
/// intelligence — module risk, test stability, release/requirement/bug history.
/// Reuses `execution_telemetry`, the quality graph and the Phase 2 entities; adds
/// no second memory store and no second flakiness engine.
pub mod qa_memory;
/// Phase 5 (QA Intelligence Memory): module and release risk, built on the
/// existing adaptive_intelligence::calibrate_risk rather than a second score.
pub mod qa_memory_risk;
pub mod scheduler;
pub mod skill_files;
pub mod state;
pub mod store;
pub mod tools;
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
#[cfg(test)]
mod engine_tests;
#[cfg(test)]
mod governed_healing_tests;
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
/// Phase 2 (QA Intelligence Memory): the persistent foundation — the durable
/// test-case <-> execution link, releases, and modules on the existing quality
/// graph. See docs/qa-intelligence-memory/PHASE_1_AUDIT.md.
#[cfg(test)]
mod qa_memory_tests;
/// Phase 3 (QA Intelligence Memory): retrieval — module risk, test stability,
/// release/requirement/bug history. One test per line of the master plan's Phase 3
/// test list, plus the §3 retrieval rules asserted directly.
#[cfg(test)]
mod qa_memory_retrieval_tests;
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
/// Phase 6 (QA Intelligence Memory): performance and storage **measurement** — index
/// usage via `EXPLAIN QUERY PLAN`, retrieval latency on a large history, signal
/// bounding, and the storage cost of one remembered execution.
#[cfg(test)]
mod qa_memory_perf_tests;
/// RBAC testing that measures the server instead of describing it: `run_probe`
/// sends a real request with a real key and reports the real answer. See
/// `rbac_probe`'s doc comment for why this is not a declared matrix.
pub mod rbac_probe;
#[cfg(test)]
mod retention_and_metrics_tests;

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
