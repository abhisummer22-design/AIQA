pub mod adaptive_intelligence;
pub mod agent;
pub mod agent_execution;
pub mod agent_tools;
pub mod agents;
pub mod ai_generation;
pub mod ai_router;
pub mod api;
pub mod api_testing;
pub mod app_source;
pub mod auth;
pub mod autopilot;
pub mod autopilot_estimate;
pub mod autopilot_store;
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
pub mod log_bus;
pub mod login_suggest;
pub mod metrics;
pub mod mobile;
pub mod operations;
pub mod performance_testing;
pub mod plan_validator;
pub mod policy;
pub mod requirement_generation;
pub mod requirements;
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
mod engine_tests;
#[cfg(test)]
mod governed_healing_tests;
#[cfg(test)]
mod operations_tests;
#[cfg(test)]
mod store_tests;
#[cfg(test)]
mod ws_tests;
