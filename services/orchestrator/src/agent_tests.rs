//! Phase 1 — Comprehensive Unit Tests for Agent Abstraction & Tool Contract.
//!
//! Exercises all 20 required acceptance criteria from Phase 1 §12 using deterministic
//! mock implementations with zero external network or AI API dependencies.

use crate::agent::{
    run_agent_loop, Agent, AgentContext, AgentError, AgentLimits, AgentOutcome, AgentStatus,
    AgentTraceHook, OperationalTraceHook, TurnDecision, TurnRecord, OPERATIONAL_TRACE_TARGET,
};
use crate::agent_tools::{
    AgentTool, AgentToolRegistry, ToolDefinition, ToolError, ToolInvocation, ToolObservation,
    ToolSafetyLevel,
};
use crate::ai_router::AiInfrastructure;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

// ------------------------------------------------------------------ Mock Tools

/// Deterministic mock tool returning a preset JSON value.
#[derive(Debug)]
struct MockEchoTool {
    name: String,
    safety_level: ToolSafetyLevel,
}

impl MockEchoTool {
    fn new(name: &str, safety_level: ToolSafetyLevel) -> Self {
        Self {
            name: name.to_string(),
            safety_level,
        }
    }
}

#[async_trait]
impl AgentTool for MockEchoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: "Mock tool for testing".to_string(),
            parameters_schema: json!({
                "type": "object",
                "properties": {
                    "echo": { "type": "string" }
                }
            }),
            output_schema: Some(json!({
                "type": "object",
                "properties": {
                    "echoed": { "type": "string" }
                }
            })),
            safety_level: self.safety_level,
            default_timeout_ms: 1000,
        }
    }

    async fn execute(
        &self,
        invocation: &ToolInvocation,
        _context: &AgentContext,
    ) -> Result<ToolObservation, ToolError> {
        let echo_val = invocation
            .arguments
            .get("echo")
            .cloned()
            .unwrap_or(Value::Null);
        Ok(ToolObservation::success(
            &invocation.call_id,
            &self.name,
            self.safety_level,
            json!({ "echoed": echo_val }),
            5,
        ))
    }
}

/// Mock tool that always produces an error.
#[derive(Debug)]
struct MockFailingTool {
    name: String,
}

#[async_trait]
impl AgentTool for MockFailingTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: "Mock tool that fails intentionally".to_string(),
            parameters_schema: json!({ "type": "object" }),
            output_schema: None,
            safety_level: ToolSafetyLevel::Diagnostic,
            default_timeout_ms: 500,
        }
    }

    async fn execute(
        &self,
        _invocation: &ToolInvocation,
        _context: &AgentContext,
    ) -> Result<ToolObservation, ToolError> {
        Err(ToolError::ExecutionFailed {
            tool: self.name.clone(),
            details: "forced mock tool failure".to_string(),
        })
    }
}

/// Mock tool that produces an oversized payload to test truncation.
#[derive(Debug)]
struct MockLargePayloadTool {
    name: String,
    byte_count: usize,
}

#[async_trait]
impl AgentTool for MockLargePayloadTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name.clone(),
            description: "Mock tool producing large payload".to_string(),
            parameters_schema: json!({ "type": "object" }),
            output_schema: None,
            safety_level: ToolSafetyLevel::ReadOnly,
            default_timeout_ms: 1000,
        }
    }

    async fn execute(
        &self,
        invocation: &ToolInvocation,
        _context: &AgentContext,
    ) -> Result<ToolObservation, ToolError> {
        let large_string = "X".repeat(self.byte_count);
        Ok(ToolObservation::success(
            &invocation.call_id,
            &self.name,
            ToolSafetyLevel::ReadOnly,
            json!({ "data": large_string }),
            10,
        ))
    }
}

// ------------------------------------------------------------------ Mock Agents

/// Mock agent with configurable step behavior.
#[derive(Debug)]
struct MockScriptedAgent {
    id: String,
    name: String,
    agent_type: String,
    script: Vec<TurnDecision>,
}

#[async_trait]
impl Agent for MockScriptedAgent {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn agent_type(&self) -> &str {
        &self.agent_type
    }

    async fn step(
        &self,
        context: &AgentContext,
        _tools: &AgentToolRegistry,
        _ai: &AiInfrastructure,
    ) -> Result<TurnDecision, AgentError> {
        let idx = context.current_turn;
        if idx < self.script.len() {
            Ok(self.script[idx].clone())
        } else {
            Ok(TurnDecision::Continue)
        }
    }
}

/// Mock trace hook that records event calls.
#[derive(Default)]
struct MockRecordingTraceHook {
    agent_start_called: AtomicBool,
    turn_start_count: AtomicUsize,
    tool_start_count: AtomicUsize,
    tool_end_count: AtomicUsize,
    turn_end_count: AtomicUsize,
    agent_end_called: AtomicBool,
}

impl AgentTraceHook for MockRecordingTraceHook {
    fn on_agent_start(&self, _context: &AgentContext) {
        self.agent_start_called.store(true, Ordering::SeqCst);
    }
    fn on_turn_start(&self, _context: &AgentContext, _turn: usize) {
        self.turn_start_count.fetch_add(1, Ordering::SeqCst);
    }
    fn on_tool_start(&self, _context: &AgentContext, _invocation: &ToolInvocation) {
        self.tool_start_count.fetch_add(1, Ordering::SeqCst);
    }
    fn on_tool_end(&self, _context: &AgentContext, _observation: &ToolObservation) {
        self.tool_end_count.fetch_add(1, Ordering::SeqCst);
    }
    fn on_turn_end(&self, _context: &AgentContext, _record: &TurnRecord) {
        self.turn_end_count.fetch_add(1, Ordering::SeqCst);
    }
    fn on_agent_end(&self, _context: &AgentContext, _outcome: &AgentOutcome) {
        self.agent_end_called.store(true, Ordering::SeqCst);
    }
}

// ------------------------------------------------------------------ Tests (20 scenarios)

#[tokio::test]
async fn test_01_agent_creation() {
    let agent = MockScriptedAgent {
        id: "agent-001".to_string(),
        name: "Test Planning Agent".to_string(),
        agent_type: "qa_planner".to_string(),
        script: vec![],
    };
    assert_eq!(agent.id(), "agent-001");
}

#[tokio::test]
async fn test_02_agent_identity() {
    let agent = MockScriptedAgent {
        id: "agent-id-42".to_string(),
        name: "Grounded Generation Agent".to_string(),
        agent_type: "test_generator".to_string(),
        script: vec![],
    };
    assert_eq!(agent.id(), "agent-id-42");
    assert_eq!(agent.name(), "Grounded Generation Agent");
    assert_eq!(agent.agent_type(), "test_generator");
}

#[tokio::test]
async fn test_03_agent_context_properties() {
    let limits = AgentLimits {
        max_turns: 5,
        max_tool_calls_total: 10,
        max_observation_bytes: 1024,
        timeout: Duration::from_secs(30),
    };
    let context = AgentContext::new(
        "task-100",
        Some("proj-alpha".to_string()),
        "sess-xyz",
        limits.clone(),
        None,
    );

    assert_eq!(context.task_id, "task-100");
    assert_eq!(context.project_id.as_deref(), Some("proj-alpha"));
    assert_eq!(context.session_id, "sess-xyz");
    assert_eq!(context.current_turn, 0);
    assert_eq!(context.total_tool_calls, 0);
    assert!(!context.trace_id.is_empty());
    assert!(!context.is_cancelled());
    assert!(!context.is_timed_out());
    assert!(context.remaining_time() > Duration::from_secs(20));
}

#[tokio::test]
async fn test_04_tool_registration() {
    let mut registry = AgentToolRegistry::new();
    let tool = Arc::new(MockEchoTool::new("echo_tool", ToolSafetyLevel::ReadOnly));

    assert!(registry.register(tool).is_ok());
    assert_eq!(registry.len(), 1);
    assert!(registry.has_tool("echo_tool"));
}

#[tokio::test]
async fn test_05_duplicate_tool_rejection() {
    let mut registry = AgentToolRegistry::new();
    let tool1 = Arc::new(MockEchoTool::new("dup_tool", ToolSafetyLevel::ReadOnly));
    let tool2 = Arc::new(MockEchoTool::new("dup_tool", ToolSafetyLevel::Diagnostic));

    assert!(registry.register(tool1).is_ok());
    let err = registry.register(tool2).unwrap_err();
    assert_eq!(err, ToolError::Duplicate("dup_tool".to_string()));
}

#[tokio::test]
async fn test_06_tool_lookup() {
    let mut registry = AgentToolRegistry::new();
    let tool = Arc::new(MockEchoTool::new("my_tool", ToolSafetyLevel::ReadOnly));
    registry.register(tool).unwrap();

    let fetched = registry.get("my_tool").unwrap();
    assert_eq!(fetched.definition().name, "my_tool");
}

#[tokio::test]
async fn test_07_unknown_tool_handling() {
    let registry = AgentToolRegistry::new();
    let err = registry.get("non_existent").unwrap_err();
    assert_eq!(err, ToolError::NotFound("non_existent".to_string()));
}

#[tokio::test]
async fn test_08_mock_tool_execution() {
    let tool = MockEchoTool::new("echo", ToolSafetyLevel::ReadOnly);
    let limits = AgentLimits::default();
    let context = AgentContext::new("t1", None, "s1", limits, None);
    let inv = ToolInvocation {
        call_id: "c1".to_string(),
        tool_name: "echo".to_string(),
        arguments: json!({ "echo": "hello world" }),
    };

    let obs = tool.execute(&inv, &context).await.unwrap();
    assert_eq!(obs.call_id, "c1");
    assert_eq!(obs.tool_name, "echo");
    assert!(obs.is_success());
    assert_eq!(obs.output.unwrap(), json!({ "echoed": "hello world" }));
}

#[tokio::test]
async fn test_09_structured_observation_schema() {
    let obs = ToolObservation::success(
        "call-99",
        "probe",
        ToolSafetyLevel::Diagnostic,
        json!({ "status": 200, "alive": true }),
        42,
    );

    assert_eq!(obs.safety_level, ToolSafetyLevel::Diagnostic);
    assert_eq!(obs.duration_ms, 42);
    assert!(!obs.is_truncated);
    assert!(obs.is_success());
}

#[tokio::test]
async fn test_10_agent_turn_lifecycle() {
    let mut registry = AgentToolRegistry::new();
    registry
        .register(Arc::new(MockEchoTool::new(
            "echo",
            ToolSafetyLevel::ReadOnly,
        )))
        .unwrap();

    let agent = MockScriptedAgent {
        id: "a1".to_string(),
        name: "test agent".to_string(),
        agent_type: "tester".to_string(),
        script: vec![
            TurnDecision::CallTools {
                invocations: vec![ToolInvocation {
                    call_id: "call-1".to_string(),
                    tool_name: "echo".to_string(),
                    arguments: json!({ "echo": "ping" }),
                }],
            },
            TurnDecision::Complete {
                output: json!({ "verdict": "success" }),
            },
        ],
    };

    let mut context = AgentContext::new("task-1", None, "s1", AgentLimits::default(), None);
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut context, &registry, &ai, None)
        .await
        .unwrap();

    assert_eq!(outcome.status, AgentStatus::Completed);
    assert_eq!(outcome.turns_taken, 2);
    assert_eq!(outcome.total_tool_calls, 1);
    assert_eq!(
        outcome.final_output.unwrap(),
        json!({ "verdict": "success" })
    );
    assert_eq!(context.history.len(), 2);
}

#[tokio::test]
async fn test_11_max_turn_protection() {
    let registry = AgentToolRegistry::new();
    let agent = MockScriptedAgent {
        id: "infinite-agent".to_string(),
        name: "infinite".to_string(),
        agent_type: "tester".to_string(),
        // Always continues
        script: vec![
            TurnDecision::Continue,
            TurnDecision::Continue,
            TurnDecision::Continue,
        ],
    };

    let mut limits = AgentLimits::default();
    limits.max_turns = 2; // Strict limit

    let mut context = AgentContext::new("t-inf", None, "s1", limits, None);
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut context, &registry, &ai, None)
        .await
        .unwrap();

    assert_eq!(outcome.status, AgentStatus::MaxTurnsReached);
    assert_eq!(outcome.turns_taken, 2);
}

#[tokio::test]
async fn test_12_tool_call_limit() {
    let mut registry = AgentToolRegistry::new();
    registry
        .register(Arc::new(MockEchoTool::new(
            "echo",
            ToolSafetyLevel::ReadOnly,
        )))
        .unwrap();

    let agent = MockScriptedAgent {
        id: "tool-heavy".to_string(),
        name: "heavy".to_string(),
        agent_type: "tester".to_string(),
        script: vec![TurnDecision::CallTools {
            invocations: vec![
                ToolInvocation {
                    call_id: "c1".to_string(),
                    tool_name: "echo".to_string(),
                    arguments: json!({}),
                },
                ToolInvocation {
                    call_id: "c2".to_string(),
                    tool_name: "echo".to_string(),
                    arguments: json!({}),
                },
                ToolInvocation {
                    call_id: "c3".to_string(),
                    tool_name: "echo".to_string(),
                    arguments: json!({}),
                },
            ],
        }],
    };

    let mut limits = AgentLimits::default();
    limits.max_tool_calls_total = 2; // Only allow 2 tool calls total

    let mut context = AgentContext::new("t-limit", None, "s1", limits, None);
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut context, &registry, &ai, None)
        .await
        .unwrap();

    assert_eq!(outcome.status, AgentStatus::Failed);
    assert!(outcome
        .error
        .unwrap()
        .contains("tool call limit (2) exceeded"));
}

#[tokio::test]
async fn test_13_observation_size_limit() {
    let mut registry = AgentToolRegistry::new();
    registry
        .register(Arc::new(MockLargePayloadTool {
            name: "big_data".to_string(),
            byte_count: 5000,
        }))
        .unwrap();

    let agent = MockScriptedAgent {
        id: "data-fetcher".to_string(),
        name: "fetcher".to_string(),
        agent_type: "tester".to_string(),
        script: vec![
            TurnDecision::CallTools {
                invocations: vec![ToolInvocation {
                    call_id: "c1".to_string(),
                    tool_name: "big_data".to_string(),
                    arguments: json!({}),
                }],
            },
            TurnDecision::Complete {
                output: json!({ "done": true }),
            },
        ],
    };

    let mut limits = AgentLimits::default();
    limits.max_observation_bytes = 200; // Small limit to trigger truncation

    let mut context = AgentContext::new("t-size", None, "s1", limits, None);
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut context, &registry, &ai, None)
        .await
        .unwrap();

    assert_eq!(outcome.status, AgentStatus::Completed);
    let obs = &context.history[0].tool_observations[0];
    assert!(obs.is_truncated);
    let val = obs.output.as_ref().unwrap();
    assert_eq!(val["_warning"], "truncated_output_exceeded_byte_limit");
}

#[tokio::test]
async fn test_14_cancellation() {
    let registry = AgentToolRegistry::new();
    let (tx, rx) = watch::channel(false);

    let agent = MockScriptedAgent {
        id: "cancel-me".to_string(),
        name: "cancelable".to_string(),
        agent_type: "tester".to_string(),
        script: vec![TurnDecision::Continue, TurnDecision::Continue],
    };

    let mut context = AgentContext::new("t-cancel", None, "s1", AgentLimits::default(), Some(rx));
    let ai = AiInfrastructure::from_env();

    // Signal cancel before run
    tx.send(true).unwrap();

    let outcome = run_agent_loop(&agent, &mut context, &registry, &ai, None)
        .await
        .unwrap();

    assert_eq!(outcome.status, AgentStatus::Cancelled);
}

#[tokio::test]
async fn test_15_timeout() {
    let registry = AgentToolRegistry::new();
    let agent = MockScriptedAgent {
        id: "timeout-agent".to_string(),
        name: "slow".to_string(),
        agent_type: "tester".to_string(),
        script: vec![TurnDecision::Continue],
    };

    let mut limits = AgentLimits::default();
    limits.timeout = Duration::from_millis(1); // Immediate timeout

    let mut context = AgentContext::new("t-timeout", None, "s1", limits, None);
    tokio::time::sleep(Duration::from_millis(5)).await;
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut context, &registry, &ai, None)
        .await
        .unwrap();

    assert_eq!(outcome.status, AgentStatus::TimedOut);
}

#[tokio::test]
async fn test_16_tool_failure_handling() {
    let mut registry = AgentToolRegistry::new();
    registry
        .register(Arc::new(MockFailingTool {
            name: "broken_tool".to_string(),
        }))
        .unwrap();

    let agent = MockScriptedAgent {
        id: "fault-tolerant-agent".to_string(),
        name: "ft".to_string(),
        agent_type: "tester".to_string(),
        script: vec![
            TurnDecision::CallTools {
                invocations: vec![ToolInvocation {
                    call_id: "c-fail".to_string(),
                    tool_name: "broken_tool".to_string(),
                    arguments: json!({}),
                }],
            },
            TurnDecision::Complete {
                output: json!({ "handled": true }),
            },
        ],
    };

    let mut context = AgentContext::new("t-fail", None, "s1", AgentLimits::default(), None);
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut context, &registry, &ai, None)
        .await
        .unwrap();

    assert_eq!(outcome.status, AgentStatus::Completed);
    let obs = &context.history[0].tool_observations[0];
    assert!(!obs.is_success());
    assert!(obs
        .output
        .as_ref()
        .unwrap_err()
        .contains("forced mock tool failure"));
}

#[tokio::test]
async fn test_17_model_failure_handling() {
    struct FailingAgent;

    #[async_trait]
    impl Agent for FailingAgent {
        fn id(&self) -> &str {
            "failing-agent"
        }
        fn name(&self) -> &str {
            "failer"
        }
        fn agent_type(&self) -> &str {
            "tester"
        }
        async fn step(
            &self,
            _context: &AgentContext,
            _tools: &AgentToolRegistry,
            _ai: &AiInfrastructure,
        ) -> Result<TurnDecision, AgentError> {
            Err(AgentError::ModelFailure(
                "simulated upstream 503 error".to_string(),
            ))
        }
    }

    let registry = AgentToolRegistry::new();
    let mut context = AgentContext::new("t-mf", None, "s1", AgentLimits::default(), None);
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&FailingAgent, &mut context, &registry, &ai, None)
        .await
        .unwrap();

    assert_eq!(outcome.status, AgentStatus::Failed);
    assert!(outcome
        .error
        .unwrap()
        .contains("simulated upstream 503 error"));
}

#[tokio::test]
async fn test_18_invalid_output_handling() {
    let registry = AgentToolRegistry::new();
    let agent = MockScriptedAgent {
        id: "bad-output-agent".to_string(),
        name: "bad-output".to_string(),
        agent_type: "tester".to_string(),
        script: vec![TurnDecision::Fail {
            reason: "malformed JSON in model response".to_string(),
        }],
    };

    let mut context = AgentContext::new("t-bad-out", None, "s1", AgentLimits::default(), None);
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut context, &registry, &ai, None)
        .await
        .unwrap();

    assert_eq!(outcome.status, AgentStatus::Failed);
    assert_eq!(
        outcome.error.as_deref(),
        Some("malformed JSON in model response")
    );
}

#[tokio::test]
async fn test_19_multiple_tools_sequential_invocation() {
    let mut registry = AgentToolRegistry::new();
    registry
        .register(Arc::new(MockEchoTool::new(
            "tool_a",
            ToolSafetyLevel::ReadOnly,
        )))
        .unwrap();
    registry
        .register(Arc::new(MockEchoTool::new(
            "tool_b",
            ToolSafetyLevel::Diagnostic,
        )))
        .unwrap();

    assert_eq!(registry.len(), 2);
    let list = registry.list();
    assert_eq!(list[0].name, "tool_a");
    assert_eq!(list[1].name, "tool_b");

    let agent = MockScriptedAgent {
        id: "multi-tool-agent".to_string(),
        name: "multi".to_string(),
        agent_type: "tester".to_string(),
        script: vec![
            TurnDecision::CallTools {
                invocations: vec![
                    ToolInvocation {
                        call_id: "c-a".to_string(),
                        tool_name: "tool_a".to_string(),
                        arguments: json!({ "echo": "val_a" }),
                    },
                    ToolInvocation {
                        call_id: "c-b".to_string(),
                        tool_name: "tool_b".to_string(),
                        arguments: json!({ "echo": "val_b" }),
                    },
                ],
            },
            TurnDecision::Complete {
                output: json!({ "done": true }),
            },
        ],
    };

    let mut context = AgentContext::new("t-multi", None, "s1", AgentLimits::default(), None);
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(&agent, &mut context, &registry, &ai, None)
        .await
        .unwrap();

    assert_eq!(outcome.status, AgentStatus::Completed);
    assert_eq!(context.history[0].tool_observations.len(), 2);
    assert_eq!(
        context.history[0].tool_observations[0]
            .output
            .as_ref()
            .unwrap(),
        &json!({ "echoed": "val_a" })
    );
    assert_eq!(
        context.history[0].tool_observations[1]
            .output
            .as_ref()
            .unwrap(),
        &json!({ "echoed": "val_b" })
    );
}

#[tokio::test]
async fn test_20_trace_hook_invocation() {
    let mut registry = AgentToolRegistry::new();
    registry
        .register(Arc::new(MockEchoTool::new(
            "echo",
            ToolSafetyLevel::ReadOnly,
        )))
        .unwrap();

    let agent = MockScriptedAgent {
        id: "traced-agent".to_string(),
        name: "traced".to_string(),
        agent_type: "tester".to_string(),
        script: vec![
            TurnDecision::CallTools {
                invocations: vec![ToolInvocation {
                    call_id: "call-traced".to_string(),
                    tool_name: "echo".to_string(),
                    arguments: json!({ "echo": "traced_val" }),
                }],
            },
            TurnDecision::Complete {
                output: json!({ "all": "good" }),
            },
        ],
    };

    let hook = Arc::new(MockRecordingTraceHook::default());
    let mut context = AgentContext::new("t-trace", None, "s1", AgentLimits::default(), None);
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(
        &agent,
        &mut context,
        &registry,
        &ai,
        Some(hook.clone() as Arc<dyn AgentTraceHook>),
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, AgentStatus::Completed);
    assert!(hook.agent_start_called.load(Ordering::SeqCst));
    assert_eq!(hook.turn_start_count.load(Ordering::SeqCst), 2);
    assert_eq!(hook.tool_start_count.load(Ordering::SeqCst), 1);
    assert_eq!(hook.tool_end_count.load(Ordering::SeqCst), 1);
    assert_eq!(hook.turn_end_count.load(Ordering::SeqCst), 2);
    assert!(hook.agent_end_called.load(Ordering::SeqCst));
}

#[tokio::test]
async fn test_21_operational_trace_hook_records_bounded_metadata() {
    let mut registry = AgentToolRegistry::new();
    registry
        .register(Arc::new(MockEchoTool::new(
            "echo",
            ToolSafetyLevel::ReadOnly,
        )))
        .unwrap();

    let agent = MockScriptedAgent {
        id: "telemetry-agent".to_string(),
        name: "Telemetry Agent".to_string(),
        agent_type: "qa_telemetry".to_string(),
        script: vec![
            TurnDecision::CallTools {
                invocations: vec![ToolInvocation {
                    call_id: "call-1".to_string(),
                    tool_name: "echo".to_string(),
                    arguments: json!({ "ping": "pong" }),
                }],
            },
            TurnDecision::Complete {
                output: json!({ "success": true }),
            },
        ],
    };

    let hook = Arc::new(OperationalTraceHook::new(
        "telemetry-agent",
        "qa_telemetry",
    ));
    let mut context = AgentContext::new(
        "task-trace-21",
        Some("project-alpha".to_string()),
        "sess-21",
        AgentLimits::default(),
        None,
    );
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(
        &agent,
        &mut context,
        &registry,
        &ai,
        Some(hook.clone() as Arc<dyn AgentTraceHook>),
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, AgentStatus::Completed);
    let events = hook.events();
    assert!(!events.is_empty());

    let event_types: Vec<&'static str> = events.iter().map(|e| e.event_type).collect();
    assert_eq!(
        event_types,
        vec![
            "agent_start",
            "turn_start",
            "tool_start",
            "tool_end",
            "turn_end",
            "turn_start",
            "turn_end",
            "agent_end"
        ]
    );

    // Verify metadata on agent_start
    let start_ev = &events[0];
    assert_eq!(start_ev.agent_id, "telemetry-agent");
    assert_eq!(start_ev.agent_type, "qa_telemetry");
    assert_eq!(start_ev.task_id, "task-trace-21");
    assert_eq!(start_ev.project_id.as_deref(), Some("project-alpha"));
    assert_eq!(start_ev.trace_id, context.trace_id);

    // Verify tool_start & tool_end metadata
    let tool_start = events.iter().find(|e| e.event_type == "tool_start").unwrap();
    assert_eq!(tool_start.tool_name.as_deref(), Some("echo"));

    let tool_end = events.iter().find(|e| e.event_type == "tool_end").unwrap();
    assert_eq!(tool_end.tool_name.as_deref(), Some("echo"));
    assert_eq!(tool_end.tool_success, Some(true));
    assert!(tool_end.duration_ms.is_some());

    // Verify agent_end metadata
    let end_ev = events.last().unwrap();
    assert_eq!(end_ev.event_type, "agent_end");
    assert_eq!(end_ev.status.as_deref(), Some("COMPLETED"));
    assert_eq!(end_ev.turns_taken, Some(2));
    assert_eq!(end_ev.total_tool_calls, Some(1));
    assert!(end_ev.error_category.is_none());
}

#[tokio::test]
async fn test_22_operational_trace_hook_excludes_sensitive_arguments_and_outputs() {
    let mut registry = AgentToolRegistry::new();
    registry
        .register(Arc::new(MockEchoTool::new(
            "echo",
            ToolSafetyLevel::ReadOnly,
        )))
        .unwrap();

    let secret_arg = "SUPER_SECRET_PASSWORD_123";
    let agent = MockScriptedAgent {
        id: "secret-agent".to_string(),
        name: "Secret Agent".to_string(),
        agent_type: "qa_security".to_string(),
        script: vec![
            TurnDecision::CallTools {
                invocations: vec![ToolInvocation {
                    call_id: "call-sec".to_string(),
                    tool_name: "echo".to_string(),
                    arguments: json!({ "password": secret_arg }),
                }],
            },
            TurnDecision::Complete {
                output: json!({ "secret_result": "HIGHLY_CONFIDENTIAL_OUTPUT_PAYLOAD" }),
            },
        ],
    };

    let hook = Arc::new(OperationalTraceHook::new("secret-agent", "qa_security"));
    let mut context = AgentContext::new(
        "task-sec",
        Some("proj-sec".to_string()),
        "sess-sec",
        AgentLimits::default(),
        None,
    );
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(
        &agent,
        &mut context,
        &registry,
        &ai,
        Some(hook.clone() as Arc<dyn AgentTraceHook>),
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, AgentStatus::Completed);
    let events = hook.events();

    // Serialize all events to JSON and ensure raw passwords/payloads never appear in trace records
    let serialized = serde_json::to_string(&events).unwrap();
    assert!(!serialized.contains(secret_arg));
    assert!(!serialized.contains("HIGHLY_CONFIDENTIAL_OUTPUT_PAYLOAD"));
}

#[tokio::test]
async fn test_23_operational_trace_hook_failure_path_sanitizes_error() {
    let registry = AgentToolRegistry::new();
    let raw_fail_reason = "database connect failed: password=super_secret_db_pass and aiqa_apikey_1234567890123456";
    let agent = MockScriptedAgent {
        id: "failing-agent".to_string(),
        name: "Failing Agent".to_string(),
        agent_type: "qa_failure".to_string(),
        script: vec![TurnDecision::Fail {
            reason: raw_fail_reason.to_string(),
        }],
    };

    let hook = Arc::new(OperationalTraceHook::new(
        "failing-agent",
        "qa_failure",
    ));
    let mut context = AgentContext::new("task-fail", None, "sess-fail", AgentLimits::default(), None);
    let ai = AiInfrastructure::from_env();

    let outcome = run_agent_loop(
        &agent,
        &mut context,
        &registry,
        &ai,
        Some(hook.clone() as Arc<dyn AgentTraceHook>),
    )
    .await
    .unwrap();

    assert_eq!(outcome.status, AgentStatus::Failed);
    let events = hook.events();
    let serialized = serde_json::to_string(&events).unwrap();

    // Verify that secrets are masked via sanitize_log_message
    assert!(!serialized.contains("super_secret_db_pass"));
    assert!(!serialized.contains("aiqa_apikey_1234567890123456"));
    assert!(serialized.contains("password=***"));
    assert!(serialized.contains("aiqa_***"));

    let end_ev = events.last().unwrap();
    assert_eq!(end_ev.event_type, "agent_end");
    assert_eq!(end_ev.status.as_deref(), Some("FAILED"));
    assert!(end_ev.error_category.is_some());
}

#[tokio::test]
async fn test_24_operational_trace_hook_buffer_bounded() {
    let capacity = 3;
    let hook = OperationalTraceHook::with_capacity("bounded-agent", "qa_bounded", capacity);
    let context = AgentContext::new("t", None, "s", AgentLimits::default(), None);

    hook.on_agent_start(&context);
    hook.on_turn_start(&context, 1);
    hook.on_turn_start(&context, 2);
    hook.on_turn_start(&context, 3);
    hook.on_turn_start(&context, 4);

    let events = hook.events();
    assert_eq!(events.len(), capacity);
    // Oldest events were evicted
    let turns: Vec<Option<usize>> = events.iter().map(|e| e.turn).collect();
    assert_eq!(turns, vec![Some(2), Some(3), Some(4)]);
}

#[test]
fn test_25_default_env_filter_permits_agent_trace_target() {
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new(
            "aiqa_orchestrator=info,tower_http=info",
        ))
        .finish();

    tracing::subscriber::with_default(subscriber, || {
        // Operational agent trace target at INFO level is enabled under default filter
        assert!(tracing::enabled!(
            target: OPERATIONAL_TRACE_TARGET,
            tracing::Level::INFO
        ));
        // Operational agent trace target at WARN level is enabled
        assert!(tracing::enabled!(
            target: OPERATIONAL_TRACE_TARGET,
            tracing::Level::WARN
        ));
        // Operational agent trace target at ERROR level is enabled
        assert!(tracing::enabled!(
            target: OPERATIONAL_TRACE_TARGET,
            tracing::Level::ERROR
        ));
        // Operational agent trace target at DEBUG level is disabled under default info filter
        assert!(!tracing::enabled!(
            target: OPERATIONAL_TRACE_TARGET,
            tracing::Level::DEBUG
        ));
        // Tower HTTP logging remains enabled under default filter
        assert!(tracing::enabled!(
            target: "tower_http",
            tracing::Level::INFO
        ));
        // Unrelated crate logging remains disabled
        assert!(!tracing::enabled!(
            target: "unrelated_crate::foo",
            tracing::Level::INFO
        ));
    });
}
