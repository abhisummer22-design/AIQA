//! Phase 1 — Native Rust Agent Abstraction, Turn Model & Execution Context.
//!
//! This module establishes the minimal, production-quality foundation for an
//! AIQA-native Agent system in pure Rust.
//!
//! Architectural Guarantees:
//! - Reuses existing `crate::ai_router::AiInfrastructure`: No second router or model catalog.
//! - Non-leaking: Private chain-of-thought is never stored or exposed.
//! - Safe Limits: Hard ceilings on max turns, max tool calls, observation size, and timeouts.
//! - Cooperative Cancellation: Integrates with Tokio `watch::Receiver<bool>`.
//! - Tracing Ready: Extensible trace hooks for Phase 4 observation logging.

use crate::agent_tools::{AgentToolRegistry, ToolError, ToolInvocation, ToolObservation};
use crate::ai_router::AiInfrastructure;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;

/// Configurable safety limits for agent execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLimits {
    /// Maximum number of iterative reasoning turns allowed.
    pub max_turns: usize,
    /// Maximum total tool invocations across all turns.
    pub max_tool_calls_total: usize,
    /// Maximum observation payload size in bytes before safe truncation occurs.
    pub max_observation_bytes: usize,
    /// Overall execution timeout duration.
    #[serde(with = "humantime_serde")]
    pub timeout: Duration,
}

mod humantime_serde {
    use serde::{self, Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(d: &Duration, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        s.serialize_u64(d.as_millis() as u64)
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ms = u64::deserialize(d)?;
        Ok(Duration::from_millis(ms))
    }
}

impl Default for AgentLimits {
    fn default() -> Self {
        Self {
            max_turns: 10,
            max_tool_calls_total: 25,
            max_observation_bytes: 32 * 1024, // 32 KB
            timeout: Duration::from_secs(60),
        }
    }
}

/// Structured decision emitted by an agent at the end of a reasoning step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TurnDecision {
    /// Agent requests further thinking or internal iteration without tools.
    Continue,
    /// Agent requests execution of one or more tools.
    CallTools { invocations: Vec<ToolInvocation> },
    /// Agent has completed its goal with structured final output.
    Complete { output: Value },
    /// Agent encountered an unrecoverable failure during reasoning.
    Fail { reason: String },
}

/// Operational metadata for an AI model interaction within a turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModelResponseMetadata {
    pub provider: String,
    pub model: String,
    pub duration_ms: u64,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

/// Operational record of a single completed agent turn.
///
/// Note: Never contains private chain-of-thought reasoning text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TurnRecord {
    pub turn_number: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_metadata: Option<ModelResponseMetadata>,
    pub tool_invocations: Vec<ToolInvocation>,
    pub tool_observations: Vec<ToolObservation>,
    pub decision: TurnDecision,
    pub duration_ms: u64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Terminal state of an agent execution run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentStatus {
    Completed,
    MaxTurnsReached,
    TimedOut,
    Cancelled,
    Failed,
}

/// Final outcome returned by agent execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentOutcome {
    pub status: AgentStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_output: Option<Value>,
    pub turns_taken: usize,
    pub total_duration_ms: u64,
    pub total_tool_calls: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Structured errors encountered during agent lifecycle or turn execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentError {
    InvalidAgent(String),
    ToolError(ToolError),
    ToolLimitExceeded(String),
    ObservationSizeLimitExceeded(String),
    ModelFailure(String),
    Cancelled(String),
    TimedOut { elapsed_ms: u64, limit_ms: u64 },
    MaxTurnsExceeded { max_turns: usize },
    InvalidOutput(String),
    ContextLimitExceeded(String),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::InvalidAgent(msg) => write!(f, "invalid agent definition: {msg}"),
            AgentError::ToolError(err) => write!(f, "tool error: {err}"),
            AgentError::ToolLimitExceeded(msg) => write!(f, "tool call limit exceeded: {msg}"),
            AgentError::ObservationSizeLimitExceeded(msg) => {
                write!(f, "observation size limit exceeded: {msg}")
            }
            AgentError::ModelFailure(msg) => write!(f, "model generation failure: {msg}"),
            AgentError::Cancelled(msg) => write!(f, "execution cancelled: {msg}"),
            AgentError::TimedOut {
                elapsed_ms,
                limit_ms,
            } => write!(
                f,
                "execution timed out after {elapsed_ms}ms (limit: {limit_ms}ms)"
            ),
            AgentError::MaxTurnsExceeded { max_turns } => {
                write!(f, "maximum turns exceeded ({max_turns})")
            }
            AgentError::InvalidOutput(msg) => write!(f, "invalid output from agent: {msg}"),
            AgentError::ContextLimitExceeded(msg) => write!(f, "context limit exceeded: {msg}"),
        }
    }
}

impl std::error::Error for AgentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AgentError::ToolError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<ToolError> for AgentError {
    fn from(err: ToolError) -> Self {
        AgentError::ToolError(err)
    }
}

/// Controlled execution context provided to an agent during its run.
pub struct AgentContext {
    pub task_id: String,
    pub project_id: Option<String>,
    pub session_id: String,
    pub trace_id: String,
    pub limits: AgentLimits,
    pub current_turn: usize,
    pub total_tool_calls: usize,
    pub cancel_rx: Option<watch::Receiver<bool>>,
    start_time: Instant,
    deadline: Instant,
    pub metadata: HashMap<String, Value>,
    pub history: Vec<TurnRecord>,
    pub auth: Option<crate::auth::AuthContext>,
    pub store: Option<crate::store::Store>,
}

impl AgentContext {
    /// Creates a new execution context initialized with limits and deadlines.
    pub fn new(
        task_id: impl Into<String>,
        project_id: Option<String>,
        session_id: impl Into<String>,
        limits: AgentLimits,
        cancel_rx: Option<watch::Receiver<bool>>,
    ) -> Self {
        let now = Instant::now();
        let deadline = now + limits.timeout;
        let trace_id = uuid::Uuid::new_v4().to_string();

        Self {
            task_id: task_id.into(),
            project_id,
            session_id: session_id.into(),
            trace_id,
            limits,
            current_turn: 0,
            total_tool_calls: 0,
            cancel_rx,
            start_time: now,
            deadline,
            metadata: HashMap::new(),
            history: Vec::new(),
            auth: None,
            store: None,
        }
    }

    /// Attaches authentication context to the execution context.
    pub fn with_auth(mut self, auth: crate::auth::AuthContext) -> Self {
        self.auth = Some(auth);
        self
    }

    /// Attaches database store to the execution context for data access.
    pub fn with_store(mut self, store: crate::store::Store) -> Self {
        self.store = Some(store);
        self
    }

    /// Creates an AI request carrying this agent's trace ID for end-to-end correlation.
    pub fn create_ai_request(
        &self,
        task: crate::ai_router::AiTask,
        prompt: impl Into<String>,
    ) -> crate::ai_router::AiRequest {
        crate::ai_router::AiRequest {
            task,
            prompt: prompt.into(),
            preferred_provider: None,
            preferred_model: None,
            trace_id: Some(self.trace_id.clone()),
        }
    }

    /// Checks if this execution context is authorized to access the specified project.
    ///
    /// Respects project scoping and RBAC:
    /// - If `context.project_id` is set, target must match.
    /// - If `context.auth` is set:
    ///   - If `auth.project_id` is set, target must match.
    ///   - If `auth.project_id` is None, caller must have `Role::Admin` to access arbitrary projects.
    pub fn is_authorized_for_project(&self, target_project: &str) -> bool {
        if let Some(ref scoped_pid) = self.project_id {
            if scoped_pid != target_project {
                return false;
            }
        }
        if let Some(ref auth) = self.auth {
            if let Some(ref auth_pid) = auth.project_id {
                if auth_pid != target_project {
                    return false;
                }
            } else if auth.role != crate::auth::Role::Admin {
                return false;
            }
        }
        true
    }

    /// Checks if cancellation has been signaled.
    pub fn is_cancelled(&self) -> bool {
        if let Some(rx) = &self.cancel_rx {
            *rx.borrow()
        } else {
            false
        }
    }

    /// Checks if execution duration has exceeded the deadline.
    pub fn is_timed_out(&self) -> bool {
        Instant::now() >= self.deadline
    }

    /// Returns remaining time before deadline.
    pub fn remaining_time(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    /// Returns elapsed duration since context start.
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Verifies that budget limits (time, turns, tool calls) are respected.
    pub fn check_limits(&self) -> Result<(), AgentError> {
        if self.is_cancelled() {
            return Err(AgentError::Cancelled(
                "cooperative cancellation requested".to_string(),
            ));
        }
        if self.is_timed_out() {
            return Err(AgentError::TimedOut {
                elapsed_ms: self.elapsed().as_millis() as u64,
                limit_ms: self.limits.timeout.as_millis() as u64,
            });
        }
        if self.current_turn >= self.limits.max_turns {
            return Err(AgentError::MaxTurnsExceeded {
                max_turns: self.limits.max_turns,
            });
        }
        if self.total_tool_calls >= self.limits.max_tool_calls_total {
            return Err(AgentError::ToolLimitExceeded(format!(
                "total tool calls ({}) reached configured limit ({})",
                self.total_tool_calls, self.limits.max_tool_calls_total
            )));
        }
        Ok(())
    }

    /// Appends a completed turn record to the context history.
    pub fn record_turn(&mut self, record: TurnRecord) {
        self.history.push(record);
        self.current_turn += 1;
    }
}

/// Extensible hook for capturing agent and tool execution events for tracing.
pub trait AgentTraceHook: Send + Sync {
    fn on_agent_start(&self, context: &AgentContext);
    fn on_turn_start(&self, context: &AgentContext, turn: usize);
    fn on_tool_start(&self, context: &AgentContext, invocation: &ToolInvocation);
    fn on_tool_end(&self, context: &AgentContext, observation: &ToolObservation);
    fn on_turn_end(&self, context: &AgentContext, record: &TurnRecord);
    fn on_agent_end(&self, context: &AgentContext, outcome: &AgentOutcome);
}

/// No-op trace hook implementation.
pub struct NoopTraceHook;

impl AgentTraceHook for NoopTraceHook {
    fn on_agent_start(&self, _context: &AgentContext) {}
    fn on_turn_start(&self, _context: &AgentContext, _turn: usize) {}
    fn on_tool_start(&self, _context: &AgentContext, _invocation: &ToolInvocation) {}
    fn on_tool_end(&self, _context: &AgentContext, _observation: &ToolObservation) {}
    fn on_turn_end(&self, _context: &AgentContext, _record: &TurnRecord) {}
    fn on_agent_end(&self, _context: &AgentContext, _outcome: &AgentOutcome) {}
}

/// Sanitized, bounded operational telemetry record for agent execution steps.
///
/// Architectural Guarantees:
/// - Never includes user prompts, model completion bodies, or private CoT.
/// - Never includes raw tool arguments or tool observation payloads.
/// - Credentials, tokens, and secrets are masked via `crate::log_bus::sanitize_log_message`.
/// - Trace ID is preserved for correlation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTraceEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: &'static str,
    pub agent_id: String,
    pub agent_type: String,
    pub task_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub trace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_safety_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_success: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision_type: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turns_taken: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tool_calls: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_category: Option<String>,
}

pub const DEFAULT_HOOK_EVENT_BUFFER_CAPACITY: usize = 128;
pub const OPERATIONAL_TRACE_TARGET: &str = "aiqa_orchestrator::agent::trace";

/// Production-grade operational telemetry hook for agent loops.
///
/// Emits structured `tracing` events and maintains a bounded in-memory event buffer.
/// Strictly non-blocking, secret-sanitizing, and free of arbitrary payloads or prompts.
#[derive(Debug, Clone)]
pub struct OperationalTraceHook {
    agent_id: String,
    agent_type: String,
    buffer_capacity: usize,
    events: Arc<Mutex<Vec<AgentTraceEvent>>>,
}

impl OperationalTraceHook {
    pub fn new(agent_id: impl Into<String>, agent_type: impl Into<String>) -> Self {
        Self::with_capacity(agent_id, agent_type, DEFAULT_HOOK_EVENT_BUFFER_CAPACITY)
    }

    pub fn with_capacity(
        agent_id: impl Into<String>,
        agent_type: impl Into<String>,
        capacity: usize,
    ) -> Self {
        Self {
            agent_id: agent_id.into(),
            agent_type: agent_type.into(),
            buffer_capacity: capacity.max(1),
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn agent_type(&self) -> &str {
        &self.agent_type
    }

    /// Returns a snapshot of events captured by this hook.
    pub fn events(&self) -> Vec<AgentTraceEvent> {
        self.events.lock().map(|g| g.clone()).unwrap_or_default()
    }

    /// Clears recorded events.
    pub fn clear(&self) {
        if let Ok(mut g) = self.events.lock() {
            g.clear();
        }
    }

    fn record_event(&self, event: AgentTraceEvent) {
        if let Ok(mut g) = self.events.lock() {
            if g.len() >= self.buffer_capacity {
                g.remove(0);
            }
            g.push(event);
        }
    }
}

impl Default for OperationalTraceHook {
    fn default() -> Self {
        Self::new("unknown_agent", "general")
    }
}

impl AgentTraceHook for OperationalTraceHook {
    fn on_agent_start(&self, context: &AgentContext) {
        tracing::info!(
            target: OPERATIONAL_TRACE_TARGET,
            agent_id = %self.agent_id,
            agent_type = %self.agent_type,
            task_id = %context.task_id,
            project_id = ?context.project_id,
            trace_id = %context.trace_id,
            "Agent execution started"
        );

        let event = AgentTraceEvent {
            timestamp: chrono::Utc::now(),
            event_type: "agent_start",
            agent_id: self.agent_id.clone(),
            agent_type: self.agent_type.clone(),
            task_id: context.task_id.clone(),
            project_id: context.project_id.clone(),
            trace_id: context.trace_id.clone(),
            turn: None,
            duration_ms: None,
            tool_name: None,
            tool_safety_level: None,
            tool_success: None,
            decision_type: None,
            status: None,
            turns_taken: None,
            total_tool_calls: None,
            error_category: None,
        };
        self.record_event(event);
    }

    fn on_turn_start(&self, context: &AgentContext, turn: usize) {
        tracing::debug!(
            target: OPERATIONAL_TRACE_TARGET,
            agent_id = %self.agent_id,
            turn = turn,
            trace_id = %context.trace_id,
            "Agent turn started"
        );

        let event = AgentTraceEvent {
            timestamp: chrono::Utc::now(),
            event_type: "turn_start",
            agent_id: self.agent_id.clone(),
            agent_type: self.agent_type.clone(),
            task_id: context.task_id.clone(),
            project_id: context.project_id.clone(),
            trace_id: context.trace_id.clone(),
            turn: Some(turn),
            duration_ms: None,
            tool_name: None,
            tool_safety_level: None,
            tool_success: None,
            decision_type: None,
            status: None,
            turns_taken: None,
            total_tool_calls: None,
            error_category: None,
        };
        self.record_event(event);
    }

    fn on_tool_start(&self, context: &AgentContext, invocation: &ToolInvocation) {
        tracing::info!(
            target: OPERATIONAL_TRACE_TARGET,
            agent_id = %self.agent_id,
            tool_name = %invocation.tool_name,
            call_id = %invocation.call_id,
            trace_id = %context.trace_id,
            "Agent tool started"
        );

        let event = AgentTraceEvent {
            timestamp: chrono::Utc::now(),
            event_type: "tool_start",
            agent_id: self.agent_id.clone(),
            agent_type: self.agent_type.clone(),
            task_id: context.task_id.clone(),
            project_id: context.project_id.clone(),
            trace_id: context.trace_id.clone(),
            turn: Some(context.current_turn + 1),
            duration_ms: None,
            tool_name: Some(invocation.tool_name.clone()),
            tool_safety_level: None,
            tool_success: None,
            decision_type: None,
            status: None,
            turns_taken: None,
            total_tool_calls: None,
            error_category: None,
        };
        self.record_event(event);
    }

    fn on_tool_end(&self, context: &AgentContext, observation: &ToolObservation) {
        let is_success = observation.output.is_ok();
        let safety_str = format!("{:?}", observation.safety_level);
        tracing::info!(
            target: OPERATIONAL_TRACE_TARGET,
            agent_id = %self.agent_id,
            tool_name = %observation.tool_name,
            duration_ms = observation.duration_ms,
            is_success = is_success,
            is_truncated = observation.is_truncated,
            trace_id = %context.trace_id,
            "Agent tool completed"
        );

        let event = AgentTraceEvent {
            timestamp: chrono::Utc::now(),
            event_type: "tool_end",
            agent_id: self.agent_id.clone(),
            agent_type: self.agent_type.clone(),
            task_id: context.task_id.clone(),
            project_id: context.project_id.clone(),
            trace_id: context.trace_id.clone(),
            turn: Some(context.current_turn + 1),
            duration_ms: Some(observation.duration_ms),
            tool_name: Some(observation.tool_name.clone()),
            tool_safety_level: Some(safety_str),
            tool_success: Some(is_success),
            decision_type: None,
            status: None,
            turns_taken: None,
            total_tool_calls: None,
            error_category: None,
        };
        self.record_event(event);
    }

    fn on_turn_end(&self, context: &AgentContext, record: &TurnRecord) {
        let decision_type = match &record.decision {
            TurnDecision::Continue => "CONTINUE",
            TurnDecision::CallTools { .. } => "CALL_TOOLS",
            TurnDecision::Complete { .. } => "COMPLETE",
            TurnDecision::Fail { .. } => "FAIL",
        };
        tracing::info!(
            target: OPERATIONAL_TRACE_TARGET,
            agent_id = %self.agent_id,
            turn = record.turn_number,
            duration_ms = record.duration_ms,
            tool_calls = record.tool_invocations.len(),
            decision_type = decision_type,
            trace_id = %context.trace_id,
            "Agent turn completed"
        );

        let event = AgentTraceEvent {
            timestamp: chrono::Utc::now(),
            event_type: "turn_end",
            agent_id: self.agent_id.clone(),
            agent_type: self.agent_type.clone(),
            task_id: context.task_id.clone(),
            project_id: context.project_id.clone(),
            trace_id: context.trace_id.clone(),
            turn: Some(record.turn_number),
            duration_ms: Some(record.duration_ms),
            tool_name: None,
            tool_safety_level: None,
            tool_success: None,
            decision_type: Some(decision_type),
            status: None,
            turns_taken: None,
            total_tool_calls: Some(record.tool_invocations.len()),
            error_category: match &record.decision {
                TurnDecision::Fail { reason } => {
                    let sanitized = crate::log_bus::sanitize_log_message(reason);
                    let bounded = sanitized.chars().take(200).collect::<String>();
                    Some(bounded)
                }
                _ => None,
            },
        };
        self.record_event(event);
    }

    fn on_agent_end(&self, context: &AgentContext, outcome: &AgentOutcome) {
        let status_str = match outcome.status {
            AgentStatus::Completed => "COMPLETED",
            AgentStatus::MaxTurnsReached => "MAX_TURNS_REACHED",
            AgentStatus::TimedOut => "TIMED_OUT",
            AgentStatus::Cancelled => "CANCELLED",
            AgentStatus::Failed => "FAILED",
        };
        let sanitized_error = outcome.error.as_ref().map(|err| {
            let s = crate::log_bus::sanitize_log_message(err);
            s.chars().take(200).collect::<String>()
        });

        if outcome.status == AgentStatus::Completed {
            tracing::info!(
                target: OPERATIONAL_TRACE_TARGET,
                agent_id = %self.agent_id,
                status = status_str,
                turns_taken = outcome.turns_taken,
                total_duration_ms = outcome.total_duration_ms,
                total_tool_calls = outcome.total_tool_calls,
                trace_id = %context.trace_id,
                "Agent execution completed"
            );
        } else {
            tracing::warn!(
                target: OPERATIONAL_TRACE_TARGET,
                agent_id = %self.agent_id,
                status = status_str,
                turns_taken = outcome.turns_taken,
                total_duration_ms = outcome.total_duration_ms,
                total_tool_calls = outcome.total_tool_calls,
                error = ?sanitized_error,
                trace_id = %context.trace_id,
                "Agent execution ended with non-completed status"
            );
        }

        let event = AgentTraceEvent {
            timestamp: chrono::Utc::now(),
            event_type: "agent_end",
            agent_id: self.agent_id.clone(),
            agent_type: self.agent_type.clone(),
            task_id: context.task_id.clone(),
            project_id: context.project_id.clone(),
            trace_id: context.trace_id.clone(),
            turn: None,
            duration_ms: Some(outcome.total_duration_ms),
            tool_name: None,
            tool_safety_level: None,
            tool_success: None,
            decision_type: None,
            status: Some(status_str.to_string()),
            turns_taken: Some(outcome.turns_taken),
            total_tool_calls: Some(outcome.total_tool_calls),
            error_category: sanitized_error,
        };
        self.record_event(event);
    }
}

/// Core native Rust Agent abstraction.
#[async_trait]
pub trait Agent: Send + Sync {
    /// Unique identifier for this agent instance.
    fn id(&self) -> &str;

    /// Human-readable name for the agent (e.g., "Grounded Test Planning Agent").
    fn name(&self) -> &str;

    /// High-level category or type identifier.
    fn agent_type(&self) -> &str;

    /// Executes a single reasoning step.
    ///
    /// Evaluates current history/context, decides whether to call tools or terminate,
    /// and returns a `TurnDecision`.
    async fn step(
        &self,
        context: &AgentContext,
        tools: &AgentToolRegistry,
        ai: &AiInfrastructure,
    ) -> Result<TurnDecision, AgentError>;
}

/// Drives an agent through controlled iterative turns up to safety ceilings.
pub async fn run_agent_loop(
    agent: &dyn Agent,
    context: &mut AgentContext,
    tools: &AgentToolRegistry,
    ai: &AiInfrastructure,
    trace_hook: Option<Arc<dyn AgentTraceHook>>,
) -> Result<AgentOutcome, AgentError> {
    let hook = trace_hook.unwrap_or_else(|| Arc::new(NoopTraceHook));
    hook.on_agent_start(context);

    let mut final_status = AgentStatus::Completed;
    let mut final_output: Option<Value> = None;
    let mut final_error: Option<String> = None;

    while context.current_turn < context.limits.max_turns {
        let turn_index = context.current_turn + 1;
        hook.on_turn_start(context, turn_index);

        if let Err(e) = context.check_limits() {
            let (status, err_str) = match &e {
                AgentError::Cancelled(msg) => (AgentStatus::Cancelled, msg.clone()),
                AgentError::TimedOut { .. } => (AgentStatus::TimedOut, e.to_string()),
                AgentError::MaxTurnsExceeded { .. } => {
                    (AgentStatus::MaxTurnsReached, e.to_string())
                }
                AgentError::ToolLimitExceeded(msg) => (AgentStatus::Failed, msg.clone()),
                _ => (AgentStatus::Failed, e.to_string()),
            };
            final_status = status;
            final_error = Some(err_str);
            break;
        }

        let turn_start = Instant::now();

        // Execute step with agent context trace ID active in task-local storage
        let decision_res = crate::auth::with_request_trace_id(
            Some(context.trace_id.clone()),
            agent.step(context, tools, ai),
        )
        .await;

        let decision = match decision_res {
            Ok(d) => d,
            Err(e) => {
                final_status = AgentStatus::Failed;
                final_error = Some(e.to_string());
                break;
            }
        };

        let mut turn_invocations = Vec::new();
        let mut turn_observations = Vec::new();

        match &decision {
            TurnDecision::Continue => {
                // Agent continues to next turn
            }
            TurnDecision::CallTools { invocations } => {
                for inv in invocations {
                    if context.total_tool_calls >= context.limits.max_tool_calls_total {
                        let err = AgentError::ToolLimitExceeded(format!(
                            "tool call limit ({}) exceeded",
                            context.limits.max_tool_calls_total
                        ));
                        final_status = AgentStatus::Failed;
                        final_error = Some(err.to_string());
                        break;
                    }

                    hook.on_tool_start(context, inv);
                    context.total_tool_calls += 1;
                    turn_invocations.push(inv.clone());

                    // Execute tool
                    let tool_res = tools.get(&inv.tool_name);
                    let mut observation = match tool_res {
                        Ok(tool) => match tool.execute(inv, context).await {
                            Ok(obs) => obs,
                            Err(te) => ToolObservation::failure(
                                &inv.call_id,
                                &inv.tool_name,
                                tool.definition().safety_level,
                                te.to_string(),
                                0,
                            ),
                        },
                        Err(te) => ToolObservation::failure(
                            &inv.call_id,
                            &inv.tool_name,
                            crate::agent_tools::ToolSafetyLevel::ReadOnly,
                            te.to_string(),
                            0,
                        ),
                    };

                    // Enforce observation size limit
                    if let Ok(val) = &observation.output {
                        let json_str = serde_json::to_string(val).unwrap_or_default();
                        if json_str.len() > context.limits.max_observation_bytes {
                            observation.is_truncated = true;
                            let truncated_preview = json_str
                                .chars()
                                .take(context.limits.max_observation_bytes)
                                .collect::<String>();
                            observation.output = Ok(serde_json::json!({
                                "_warning": "truncated_output_exceeded_byte_limit",
                                "raw_preview": truncated_preview,
                            }));
                        }
                    }

                    hook.on_tool_end(context, &observation);
                    turn_observations.push(observation);
                }

                if final_status == AgentStatus::Failed {
                    break;
                }
            }
            TurnDecision::Complete { output } => {
                final_status = AgentStatus::Completed;
                final_output = Some(output.clone());
            }
            TurnDecision::Fail { reason } => {
                final_status = AgentStatus::Failed;
                final_error = Some(reason.clone());
            }
        }

        let record = TurnRecord {
            turn_number: turn_index,
            model_metadata: None,
            tool_invocations: turn_invocations,
            tool_observations: turn_observations,
            decision: decision.clone(),
            duration_ms: turn_start.elapsed().as_millis() as u64,
            timestamp: chrono::Utc::now(),
        };

        hook.on_turn_end(context, &record);
        context.record_turn(record);

        if matches!(
            decision,
            TurnDecision::Complete { .. } | TurnDecision::Fail { .. }
        ) {
            break;
        }
    }

    if final_status == AgentStatus::Completed
        && final_output.is_none()
        && context.current_turn >= context.limits.max_turns
    {
        final_status = AgentStatus::MaxTurnsReached;
    }

    let outcome = AgentOutcome {
        status: final_status,
        final_output,
        turns_taken: context.current_turn,
        total_duration_ms: context.elapsed().as_millis() as u64,
        total_tool_calls: context.total_tool_calls,
        error: final_error,
    };

    hook.on_agent_end(context, &outcome);
    Ok(outcome)
}
