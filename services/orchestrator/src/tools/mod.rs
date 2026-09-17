//! Phase 2 — Safe QA Diagnostic Tools.
//!
//! Provides the first set of production-quality, read-only and diagnostic
//! `AgentTool` implementations for AIQA agents:
//! - `schema_inspector`: Safely inspects OpenAPI/Swagger schemas.
//! - `dom_prober`: Diagnostic inspection of crawler DOM snapshots.
//! - `requirement_lookup`: Multi-tenant requirement and quality-graph retrieval.
//!
//! Architectural Boundary:
//! - Strictly ReadOnly and Diagnostic safety levels.
//! - SSRF protected via `crate::executor::validate_target_url`.
//! - No browser launch, no Playwright execution, no arbitrary code evaluation.
//! - Completely decoupled from downstream `ExecutorRegistry` and test execution.

pub mod dom_prober;
pub mod requirement_lookup;
pub mod schema_inspector;
pub mod security;

#[cfg(test)]
mod tests;

pub use dom_prober::DomProberTool;
pub use requirement_lookup::RequirementLookupTool;
pub use schema_inspector::SchemaInspectorTool;

use crate::agent_tools::{AgentToolRegistry, ToolError};
use crate::store::Store;
use std::sync::Arc;

/// Registers all Phase 2 diagnostic tools into an `AgentToolRegistry`.
pub fn register_diagnostic_tools(
    registry: &mut AgentToolRegistry,
    store: Option<Store>,
) -> Result<(), ToolError> {
    registry.register(Arc::new(SchemaInspectorTool::new()))?;
    registry.register(Arc::new(DomProberTool::new()))?;
    registry.register(Arc::new(RequirementLookupTool::new(store)))?;
    Ok(())
}
