# AIQA Native Agent Foundation — Developer Guide (Phase 1)

This guide documents the native Rust Agent abstraction introduced in Phase 1 of the AIQA architecture evolution.

---

## 1. What "Agent" Means in AIQA

In AIQA, an **Agent** (`crate::agent::Agent`) is an autonomous, multi-turn reasoning entity implemented natively in Rust.
Unlike one-shot prompt generation, an Agent operates in controlled iterative turns:
1. Receives an initial request and execution context (`AgentContext`).
2. Evaluates the current situation, past observations, and requirements.
3. Decides whether to:
   - Continue internal thinking (`TurnDecision::Continue`).
   - Request tool invocations (`TurnDecision::CallTools`).
   - Complete the task with structured output (`TurnDecision::Complete`).
   - Terminate due to an unrecoverable failure (`TurnDecision::Fail`).

Future specialized QA agents (Test Planning Agent, Grounded Generation Agent, Self-Healing Agent, Bug Analysis Agent) will implement this core trait.

---

## 2. What "AgentTool" Means

An **AgentTool** (`crate::agent_tools::AgentTool`) is a side-effect-controlled capability exposed to an Agent *during its reasoning process*.
Every tool defines:
- A unique, alphanumeric identifier (`name`).
- A clear intent and usage description (`description`).
- A JSON Schema for parameter validation (`parameters_schema`).
- An explicit safety classification (`ToolSafetyLevel`).
- A timeout ceiling (`default_timeout_ms`).

Tools are registered in and dispatched via the `AgentToolRegistry`.

---

## 3. Difference Between AgentTool and Executor

| Aspect | `AgentTool` (`agent_tools.rs`) | `Executor` (`executor.rs`) |
| :--- | :--- | :--- |
| **Purpose** | Diagnostic & exploratory probing during reasoning | Concrete end-to-end test execution |
| **Invocation** | Called interactively *inside* an agent turn | Dispatched downstream by the DAG scheduler |
| **Scope** | DOM inspection, schema querying, selector verification | Playwright, Newman, Pytest, Maestro, Schemathesis |
| **Safety** | Read-only or diagnostic by default; closed AST | Sandboxed OS subprocess or native HTTP runner |
| **Registry** | `AgentToolRegistry` | `ExecutorRegistry` |

The two registries are completely independent and must never be merged.

---

## 4. How Agent Uses AiInfrastructure

The Agent framework integrates with AIQA's existing central AI layer (`crate::ai_router::AiInfrastructure`):
```text
Agent.step()
    │
    ▼
AiInfrastructure::generate(AiRequest)
    │
    ├── Provider Discovery (Ollama, MJ Gateway, Groq, Gemini, Custom)
    ├── Capability Scoring & Task Overrides
    └── Multi-Provider Fallback Chain
```
There is **NO secondary model router**, NO secondary provider catalog, and NO external Python engine.

---

## 5. Safety Limits & Guardrails

To prevent infinite loops, context window exhaustion, and runaway operations, every agent run is bounded by `AgentLimits`:
- **`max_turns`**: Hard ceiling on reasoning iterations (default: 10).
- **`max_tool_calls_total`**: Maximum tool executions across the entire run (default: 25).
- **`max_observation_bytes`**: Maximum output size per tool (default: 32 KB). Oversized outputs are safely truncated with `is_truncated = true`.
- **`timeout`**: Wall-clock deadline for the context.
- **Cooperative Cancellation**: Monitored via Tokio `watch::Receiver<bool>`.
- **Zero Chain-of-Thought Persistence**: Private internal monologue is never written to disk or exposed in API payloads.

---

## 6. Why OpenJarvis Is NOT a Dependency

- **Language Mismatch**: OpenJarvis is a Python application framework; AIQA is a high-performance native Rust service.
- **Domain Mismatch**: OpenJarvis focuses on personal assistant tasks (calendar, weather, speech, generic bash). AIQA focuses on deterministic software QA orchestration.
- **Architecture Integrity**: AIQA adopts the *design patterns* (ReAct loop, tool contracts, token tracing) implemented cleanly in native Rust, maintaining zero Python runtime overhead.

---

## 7. How Future QA Agents Will Build on This Foundation

In Phase 2 and beyond:
1. **Diagnostic Tools (Phase 2)**: Adding read-only tools like `schema_inspector`, `dom_prober`, and `endpoint_verifier`.
2. **QA Agents (Phase 3)**:
   - *Test Planning Agent*: Breaks large epics into testable suites using schema tools.
   - *Self-Healing Agent*: Diagnoses Playwright selector failures and updates execution plans.
3. **Observability (Phase 4)**: Connecting `AgentTraceHook` to a persistent token and step trace store.
