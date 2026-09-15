//! Phase 1 — Native Rust Agent Tool Abstraction & Contract.
//!
//! This module defines the foundational `AgentTool` interface, safety classification,
//! invocation contract, and `AgentToolRegistry` for AIQA.
//!
//! Architectural Rules:
//! - `AgentToolRegistry` != `ExecutorRegistry`: This layer is for interactive,
//!   read-only or diagnostic reasoning during agent turns. `ExecutorRegistry`
//!   (`executor.rs`) remains exclusively responsible for downstream test execution.
//! - Closed Safety Boundaries: Tools explicitly declare their `ToolSafetyLevel`.
//! - No Arbitrary Execution: No shell execution, no arbitrary code eval.
//! - Fully typed errors and structured observations (never stringly-typed).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

/// Explicit safety classification for agent tools (Phase 1 §5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolSafetyLevel {
    /// Safe, side-effect-free information retrieval (e.g. read schema, lookup test case).
    ReadOnly,
    /// Safe, isolated verification or probing (e.g. selector check, ping, dry-run).
    Diagnostic,
    /// Modifies non-critical state or requires elevated permissions.
    Mutating,
    /// High-risk or destructive operation requiring explicit policy approval.
    Dangerous,
}

impl ToolSafetyLevel {
    /// Returns true if this tool is safe to execute without human or policy escalation.
    pub fn is_autonomous_safe(&self) -> bool {
        matches!(
            self,
            ToolSafetyLevel::ReadOnly | ToolSafetyLevel::Diagnostic
        )
    }
}

/// Structured error representing tool failures and registry rejections.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ToolError {
    NotFound(String),
    Duplicate(String),
    InvalidDefinition(String),
    InvalidInput { tool: String, details: String },
    ExecutionFailed { tool: String, details: String },
    Timeout { tool: String, timeout_ms: u64 },
    SafetyViolation { tool: String, reason: String },
    Cancelled,
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::NotFound(name) => write!(f, "tool not found: {name}"),
            ToolError::Duplicate(name) => write!(f, "duplicate tool registration: {name}"),
            ToolError::InvalidDefinition(msg) => write!(f, "invalid tool definition: {msg}"),
            ToolError::InvalidInput { tool, details } => {
                write!(f, "invalid input for tool {tool}: {details}")
            }
            ToolError::ExecutionFailed { tool, details } => {
                write!(f, "tool {tool} execution failed: {details}")
            }
            ToolError::Timeout { tool, timeout_ms } => {
                write!(f, "tool {tool} timed out after {timeout_ms}ms")
            }
            ToolError::SafetyViolation { tool, reason } => {
                write!(f, "tool safety violation for {tool}: {reason}")
            }
            ToolError::Cancelled => write!(f, "tool execution cancelled"),
        }
    }
}

impl std::error::Error for ToolError {}

/// Metadata and JSON schema contract describing an `AgentTool`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Unique, stable tool identifier (e.g., "schema_inspector", "selector_verifier").
    pub name: String,
    /// Clear description of what the tool does and when to call it.
    pub description: String,
    /// JSON Schema describing the expected parameters object.
    pub parameters_schema: Value,
    /// Optional JSON Schema describing the structured output object.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_schema: Option<Value>,
    /// Explicit safety level for governance and policy gates.
    pub safety_level: ToolSafetyLevel,
    /// Default execution timeout in milliseconds (0 = use context default).
    pub default_timeout_ms: u64,
}

impl ToolDefinition {
    /// Validates that the tool definition conforms to structural rules.
    pub fn validate(&self) -> Result<(), ToolError> {
        if self.name.trim().is_empty() {
            return Err(ToolError::InvalidDefinition(
                "tool name cannot be empty".to_string(),
            ));
        }
        if !self
            .name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(ToolError::InvalidDefinition(format!(
                "tool name '{}' must contain only alphanumeric, underscore, or hyphen characters",
                self.name
            )));
        }
        if self.description.trim().is_empty() {
            return Err(ToolError::InvalidDefinition(format!(
                "tool '{}' must provide a non-empty description",
                self.name
            )));
        }
        if !self.parameters_schema.is_object() {
            return Err(ToolError::InvalidDefinition(format!(
                "tool '{}' parameters_schema must be a JSON Schema object",
                self.name
            )));
        }
        Ok(())
    }
}

/// Request to invoke a specific tool with arguments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolInvocation {
    /// Unique identifier for this invocation instance (used to correlate observation).
    pub call_id: String,
    /// Target tool name.
    pub tool_name: String,
    /// Invocation arguments matching the tool's parameter schema.
    pub arguments: Value,
}

/// Structured outcome resulting from tool execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolObservation {
    /// Matches the `call_id` of the invocation.
    pub call_id: String,
    /// Tool name that produced this observation.
    pub tool_name: String,
    /// Safety classification of the executed tool.
    pub safety_level: ToolSafetyLevel,
    /// Structured result payload on success, or structured error details on failure.
    pub output: Result<Value, String>,
    /// Execution duration in milliseconds.
    pub duration_ms: u64,
    /// True if the output exceeded size limits and was safely truncated.
    pub is_truncated: bool,
}

impl ToolObservation {
    /// Returns true if the tool execution succeeded.
    pub fn is_success(&self) -> bool {
        self.output.is_ok()
    }

    /// Creates a successful observation.
    pub fn success(
        call_id: impl Into<String>,
        tool_name: impl Into<String>,
        safety_level: ToolSafetyLevel,
        value: Value,
        duration_ms: u64,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            tool_name: tool_name.into(),
            safety_level,
            output: Ok(value),
            duration_ms,
            is_truncated: false,
        }
    }

    /// Creates a failure observation without throwing.
    pub fn failure(
        call_id: impl Into<String>,
        tool_name: impl Into<String>,
        safety_level: ToolSafetyLevel,
        error_message: impl Into<String>,
        duration_ms: u64,
    ) -> Self {
        Self {
            call_id: call_id.into(),
            tool_name: tool_name.into(),
            safety_level,
            output: Err(error_message.into()),
            duration_ms,
            is_truncated: false,
        }
    }
}

/// Pure Rust trait for tools that can be invoked during agent reasoning turns.
#[async_trait]
pub trait AgentTool: Send + Sync + std::fmt::Debug {
    /// Returns the static metadata and schema definition for this tool.
    fn definition(&self) -> ToolDefinition;

    /// Executes the tool asynchronously against an invocation request.
    ///
    /// The context parameter provides read-only access to execution limits,
    /// project scope, and cancellation signals.
    async fn execute(
        &self,
        invocation: &ToolInvocation,
        context: &crate::agent::AgentContext,
    ) -> Result<ToolObservation, ToolError>;
}

/// Registry of available tools for Agent turns.
///
/// Distinct from `ExecutorRegistry`: This controls diagnostic and reasoning tools,
/// not external end-to-end test execution runners.
#[derive(Default, Clone)]
pub struct AgentToolRegistry {
    tools: HashMap<String, Arc<dyn AgentTool>>,
}

impl AgentToolRegistry {
    /// Creates a new, empty tool registry.
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Registers a tool into the registry. Rejects duplicates and invalid definitions.
    pub fn register(&mut self, tool: Arc<dyn AgentTool>) -> Result<(), ToolError> {
        let def = tool.definition();
        def.validate()?;

        if self.tools.contains_key(&def.name) {
            return Err(ToolError::Duplicate(def.name));
        }

        self.tools.insert(def.name, tool);
        Ok(())
    }

    /// Looks up a registered tool by its exact name.
    pub fn get(&self, name: &str) -> Result<Arc<dyn AgentTool>, ToolError> {
        self.tools
            .get(name)
            .cloned()
            .ok_or_else(|| ToolError::NotFound(name.to_string()))
    }

    /// Unregisters a tool by name, returning it if it was present.
    pub fn unregister(&mut self, name: &str) -> Option<Arc<dyn AgentTool>> {
        self.tools.remove(name)
    }

    /// Checks whether a tool is registered.
    pub fn has_tool(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Returns definitions of all registered tools, sorted stably by name.
    pub fn list(&self) -> Vec<ToolDefinition> {
        let mut defs: Vec<ToolDefinition> = self.tools.values().map(|t| t.definition()).collect();
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}
