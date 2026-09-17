# AIQA Phase 0 Architecture Audit Report
## Evaluation of OpenJarvis Architectural Concepts for Native AIQA Incorporation

- **Date**: 2026-09-13
- **Auditor**: Antigravity Core Architectural Agent
- **Status**: Phase 0 Complete — Strictly Read-Only (Audit, Compare, Analyze, Recommend)
- **AIQA Baseline**: Branch `Demo`, Commit `0a67da3`, Working Tree Clean
- **AIQA Primary Path**: `G:\AIQA`
- **OpenJarvis Reference Path**: `G:\OpenJarvis`

---

## 1. Executive Summary

This architecture audit evaluates whether architectural concepts from the **OpenJarvis** project (Stanford Hazy Research / Scaling Intelligence Lab) should be incorporated into the **AIQA** enterprise quality-orchestration platform, without damaging, duplicating, or replacing existing AIQA systems.

### Key Findings
1. **Domain Divergence**: OpenJarvis is an on-device, local-first **Personal AI Assistant** framework (voice briefings, Apple calendar, weather, generic bash, personal desktop automation) written in **Python** with PyO3 acceleration. AIQA is a specialized, production-grade **Autonomous Software Quality Assurance Orchestrator** written in **native Rust (Axum/Tokio)** with an enterprise React frontend.
2. **Direct Integration Is Disastrous**: Installing OpenJarvis as a dependency, running it as a sidecar process, or wrapping it in Python subprocesses would introduce massive runtime overhead (Python interpreter, heavy PyTorch/uv lockfiles, cross-process IPC latency, memory bloat) and create catastrophic duplication across AI routing, model discovery, SQLite storage, and execution engines.
3. **AIQA Already Surpasses OpenJarvis in QA Domains**: AIQA already features an exceptionally mature 4,690-line capability-based AI Router (`ai_router.rs`), a 3,236-line multi-wave grounded test case generator (`ai_generation.rs`), a deterministic + AI AST test compiler (`execution_plan.rs`), an empirical quality graph (`intelligence.rs`), and an enterprise DAG scheduler (`engine.rs`) controlling real test executors (Playwright, Newman, Pytest, Maestro, Schemathesis, CompiledApi).
4. **Valuable OpenJarvis Concepts to Adopt Natively in Rust**:
   - **Multi-Turn Autonomous Agent Loop**: OpenJarvis's ReAct / CodeAct loop (`Thought -> Tool Call -> Observation -> Reflection -> Output`) is superior to AIQA's current one-shot/wave prompting pattern.
   - **First-Class Tool-Calling Abstraction for Agents**: Allowing LLMs to dynamically invoke QA-specific tools (e.g., query schema, inspect DOM, validate assertion, execute single step) during reasoning.
   - **Unified Step & Token Trace System**: Structured execution tracing capturing prompts, token usage, latency, tool calls, and model decisions for auditability and optimization.
   - **Agent Checkpointing & Memory Compaction**: Session memory management that summarizes past observations to prevent context window explosion.

### Definitive Recommendation
- **Direct Integration / Dependency / Sidecar**: **NO**.
- **Adopt Architectural Patterns Natively in Rust**: **YES**.

---

## 2. Current AIQA Architecture Audit

### 2.1 Repository Structure
```text
G:\AIQA
├── forentend/                # React 19 + TypeScript + Vite + TailwindCSS v4
│   └── src/
│       ├── components/       # Pages, layout, dashboard, orchestrator, test-generator
│       ├── context/          # AIQA global React context
│       ├── routes/           # React Router v7 routes
│       └── services/         # API clients (aiqaApi.ts, skillRegistry.ts)
├── services/
│   └── orchestrator/         # Rust backend service (Axum, Tokio, SQLx)
│       ├── src/              # 32 modules (ai_router, ai_generation, engine, executor, etc.)
│       ├── migrations/       # 14 SQLx migrations (SQLite WAL mode)
│       └── skills/           # Filesystem SKILL.md repository
├── executors/                # Concrete QA tool execution fixtures
│   ├── playwright/           # Browser E2E & crawler fixtures
│   ├── newman/               # Postman collection runner fixtures
│   ├── pytest/               # Python test fixtures
│   ├── maestro/              # Mobile flow fixtures (flow.yaml)
│   └── schemathesis/         # OpenAPI property-based testing fixtures
└── infra/
    └── data/                 # SQLite database (orchestrator.sqlite3) & evidence files
```

### 2.2 Subsystem Analysis
| Subsystem | Actual Path | Responsibility | Public Interface | Persistence | Limitations |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Web & API Layer** | `services/orchestrator/src/api.rs` (5,523 lines) | REST & WebSocket API, route definitions, project scoping | Axum router on `:8080`, 50+ endpoints | None (stateless handlers) | Large single file; handler complexity is high |
| **AI Infrastructure** | `services/orchestrator/src/ai_router.rs` (4,690 lines) | Provider discovery, capability matching, auto/manual routing, fallback | `AiInfrastructure::route()`, `AiInfrastructure::generate()` | `ai_settings`, `custom_ai_providers` | No streaming tokens endpoint yet |
| **AI Generation** | `services/orchestrator/src/ai_generation.rs` (3,046 lines) | Wave-based test generation, prompt building, grounding, dedup | `generate_test_cases()` | `test_cases` table | Pipeline is linear waves; lacks multi-turn self-correction |
| **Execution Planner** | `services/orchestrator/src/execution_plan.rs` (849 lines) | Compiles natural language cases to executable AST (`Action`) | `compile()`, `materialize_task_def()` | None (in-memory plan serialization) | Limited to API and Browser actions (no native mobile AST yet) |
| **Execution Engine** | `services/orchestrator/src/engine.rs` (854 lines) | Orchestrator state machine, DAG execution, cancellation, recovery | `spawn_run()`, `recover_incomplete_runs()` | `runs`, `tasks` tables | Does not dynamically adjust DAG based on mid-run observations |
| **Executor Framework** | `services/orchestrator/src/executor.rs` (1,717 lines) | Process isolation, timeout enforcement, test tool execution | `ExecutorRegistry`, `Executor::execute()` | Writes stdout/stderr to `evidence` | Subprocesses spawn per task; no persistent browser daemon |
| **Quality Intelligence** | `services/orchestrator/src/intelligence.rs` (1,301 lines) | Risk scoring, failure clustering, flakiness detection, requirement graph | `ingest_run()`, `generate_ai_hypothesis()` | `quality_nodes`, `quality_relations`, `failure_clusters` | Clustering is deterministic regex-based; AI hypothesis is on-demand |
| **Storage Layer** | `services/orchestrator/src/store.rs` (2,799 lines) | SQLite connection pool, CRUD, migrations | `Store` struct with async SQLx methods | `orchestrator.sqlite3` | SQLite WAL single-writer concurrency limits |
| **Auth & Governance** | `services/orchestrator/src/auth.rs` (259 lines) | Bearer token authentication, RBAC (`admin`, `operator`, `viewer`) | `auth_middleware`, `require_role()` | `api_keys`, `audit_log` | Token-based only (no OIDC/SSO integration yet) |
| **Policy Engine** | `services/orchestrator/src/policy.rs` (144 lines) | Approval thresholds, environment guardrails | `PolicyConfig`, `evaluate()` | None (env-configured) | Hardcoded rule structure |
| **Evidence Store** | `services/orchestrator/src/evidence.rs` (83 lines) | Filesystem evidence storage with SHA-256 integrity checks | `LocalEvidenceStore` | Files under `infra/data/evidence/` | Local filesystem only (no S3/GCS remote adapter) |
| **Mobile Testing** | `services/orchestrator/src/mobile.rs` (772 lines) | Android adb, AVD boot, iOS simctl, Maestro execution | `/api/v1/mobile/*` endpoints | None | Requires local Android SDK / Xcode installed |
| **Performance Testing**| `services/orchestrator/src/performance_testing.rs` (1,759 lines) | HTTP load generator, latency percentiles, error rate tracking | `/api/v1/projects/:id/performance-runs` | `performance_runs` table | In-process HTTP load generator; single-node only |

---

## 3. Current AIQA AI Architecture

### 3.1 Flow of an AI Request
```text
1. User Request (e.g. POST /generate-ai-test-cases)
   ├── Input: requirement text, selected skills, crawl data, project_id
   v
2. Context Assembly (ai_generation.rs)
   ├── Resolve SKILL.md / DB skills (resolve_skill_instructions)
   ├── Partition skills into dependency waves: Base -> Dependent -> NonFunctional
   ├── Fetch past flakiness & defect patterns (intelligence::load_feedback_context)
   ├── Fetch approved case embeddings for RAG retrieval (rank_similar_test_cases)
   └── Incorporate crawler DOM tree & role visibility matrix (crawler::CrawlOutput)
   v
3. Model Routing & Execution (ai_router.rs)
   ├── Task Classified as AiTask::TestCaseGeneration
   ├── AiInfrastructure::route() evaluates:
   │   ├── Mode: AUTO vs MANUAL
   │   ├── Task Overrides (from ai_settings)
   │   ├── Capabilities: structured_output, reasoning, context_window
   │   ├── Health & Latency: Ollama, MJ Gateway, Groq, Gemini, Custom providers
   │   └── Candidate Scoring & Fallback Chain
   v
4. Model Invocation
   ├── JSON Schema Enforcement
   └── Provider Fallback on timeout/error (if fallback_enabled)
   v
5. Post-Processing & Validation (ai_generation.rs)
   ├── Serde JSON Deserialization into GeneratedTestCaseDraft
   ├── Quality Cleaning (validate_and_clean)
   ├── Traceability Gate (enforce_traceability: verify against source requirement)
   └── Wave Deduplication (deduplicate)
   v
6. Persistence (store.rs)
   └── Saved as TestCaseRow in DRAFT / GENERATED status
```

### 3.2 AI Capability Classification Matrix
| Mechanism | Status | Evidence in Codebase |
| :--- | :--- | :--- |
| **Request Classification** | **EXISTS** | `AiTask` enum (`TestCaseGeneration`, `BugAnalysis`, `CompilationPlan`, `General`) in `ai_router.rs:245`. |
| **Model/Provider Selection** | **EXISTS** | `AiInfrastructure::route()` with scoring algorithm evaluating latency, capability weights, and health in `ai_router.rs:880-1050`. |
| **Capability Evaluation** | **EXISTS** | Strongly-typed `ModelCapabilities` with `CapabilityLevel` (`Unknown`, `None`, `Basic`, `Strong`) in `ai_router.rs:72-120`. |
| **Multi-Provider Fallback** | **EXISTS** | Automatic fallback chain upon HTTP error/timeout in `ai_router.rs:1140-1280`. |
| **Structured Output** | **EXISTS** | Schema validation via serde into `GeneratedTestCaseDraft` and `ExecutionPlan` in `ai_generation.rs` and `execution_plan.rs`. |
| **Grounding & Traceability** | **EXISTS** | `enforce_traceability()` strips references not verified as exact substrings of requirement text in `ai_generation.rs:450-490`. |
| **Semantic Vector Search** | **EXISTS** | SQLite `test_case_embeddings` table + Gemini Embeddings + Rust cosine similarity in `0014_test_case_embeddings.sql` & `ai_generation.rs:2400-2550`. |
| **Agent-Like Behavior** | **PARTIALLY EXISTS** | Wave-based sequential prompting with inter-wave context passing exists, but NO autonomous multi-turn ReAct loop (Thought -> Action -> Observation). |
| **Task Planning & Decomposition**| **PARTIALLY EXISTS** | Skill waves (Base -> Dependent -> NonFunctional) and Test Case AST compilation (`execution_plan.rs`) exist, but dynamic real-time agent planning does not. |
| **Feedback / Learning Loop** | **PARTIALLY EXISTS** | Past run flakiness, defect clusters, and approved test cases feed back into generation prompts, but no automated weight tuning or self-optimization. |

---

## 4. Current AIQA Skill System Audit

### 4.1 Architecture
AIQA supports two parallel skill sources:
1. **Filesystem Skills (`skill_files.rs`)**:
   - Dropped as `SKILL.md` in `services/orchestrator/skills/`.
   - YAML frontmatter parsed for `name`, `description`, `metadata`.
   - Markdown body parsed as `instruction` text.
   - Re-scanned on every request (`scan_skills_dir`), making additions live instantly without server restart.
2. **Database Custom Skills (`store.rs`, migration `0009_custom_skills.sql`)**:
   - Managed via REST API (`POST /api/v1/skills`).
   - Stored in SQLite `skills` table with user metadata.

### 4.2 Detailed Skill Evaluation
- **Skill Discovery**: Scans filesystem directory + queries SQLite database.
- **Skill Instructions**: Injected directly into generation prompts (`SkillInput.description`).
- **Skill-to-Model Relationship**: Injected as instructional context to guide generation; skills do not select models (model selected by `AiInfrastructure`).
- **Skill-to-Executor Relationship**: Indirect; skills guide the generated test steps which later compile into executor tasks.
- **Can Skills Invoke Tools?**: **NO**. In AIQA, skills are prompt templates/instructions, NOT executable agents that call tools.
- **Can Skills Chain?**: **PARTIALLY**. Skills execute across predefined waves (Base, Dependent, NonFunctional), with later waves seeing the output of prior waves.
- **Can Skills Maintain State?**: **NO**. Skills are purely stateless.
- **Dynamically Reloadable?**: **YES**. Directory is scanned dynamically on demand.

---

## 5. Current AIQA Executor / Tool Architecture

### 5.1 Executor Contract & Registry
Defined in `executor.rs`:
```rust
#[async_trait]
pub trait Executor: Send + Sync {
    async fn execute(&self, task: &TaskDef) -> ExecutionOutcome;
}
```
Registered in `main.rs` via `ExecutorRegistry`:
1. `playwright`: Subprocess invocation (`npx playwright test`) with crawler, self-healing, and role-gated fixtures.
2. `newman`: Subprocess invocation (`newman run`) executing Postman collections.
3. `pytest`: Subprocess invocation (`pytest`) executing Python test suites.
4. `maestro`: Subprocess invocation (`maestro test`) running mobile flows.
5. `schemathesis`: Subprocess invocation (`schemathesis run`) executing property-based API tests against OpenAPI specs.
6. `compiled-api`: **Native Rust executor** using `reqwest` for compiled API test plans (zero subprocess overhead).
7. `simulated`: In-memory deterministic simulator for unit/integration testing and demo runs.

### 5.2 Process Management & Isolation
- Subprocesses executed via `tokio::process::Command` with `kill_on_drop(true)`.
- Cooperative cancellation monitored via `tokio::sync::watch::Receiver<bool>`.
- Full stdout/stderr captured and stored in `LocalEvidenceStore` (`evidence.rs`) with SHA-256 digests.
- SSRF prevention via `executor::validate_target_url` (blocks loopback/internal addresses unless explicitly permitted).

### 5.3 Comparison to an Agent "Tool Layer"
AIQA has an **Execution Layer for Test Runners**, but it does NOT have an **Agent Tool Layer**. In AIQA, tools are invoked downstream by the scheduler after test plans are generated. The LLM cannot invoke tools *during* its reasoning process to probe endpoints, inspect UI elements, or run intermediate code.

---

## 6. Current AIQA Memory / Trace / Learning Audit

| Memory / Learning Subsystem | Status | Implementation Details in AIQA |
| :--- | :--- | :--- |
| **Short-Term Memory** | **PARTIALLY EXISTS** | Per-run state held in `RunRecord` RwLock; wave-to-wave prompt accumulation in `ai_generation.rs`. |
| **Long-Term Memory** | **EXISTS** | SQLite tables (`runs`, `tasks`, `findings`, `failure_clusters`, `test_cases`, `audit_log`). |
| **Semantic Retrieval (RAG)** | **EXISTS** | Migration `0014_test_case_embeddings.sql` + Gemini Embeddings + Rust cosine similarity ranking approved test cases. |
| **Execution Traces** | **PARTIALLY EXISTS** | Realtime `RunEvent` broadcast stream (`/events/ws`) and evidence files. Lacks fine-grained LLM token/step trace graphs. |
| **Historical Test Knowledge** | **EXISTS** | `intelligence.rs` computes flakiness scores, defect clusters, and requirement impact matrices across past runs. |
| **Learning from Past Runs** | **PARTIALLY EXISTS** | `load_feedback_context()` injects past defect clusters into generation prompts. |
| **Strategy Optimization** | **NOT IMPLEMENTED** | No automated prompt refinement, DSPy optimization, or model parameter tuning based on test pass rates. |
| **Human Feedback Loop** | **EXISTS** | Finding reviews (`CONFIRMED`, `FALSE_POSITIVE`), test case approval workflow (`APPROVED`, `REJECTED`), run gate approvals. |

---

## 7. OpenJarvis Complete Architecture Audit

### 7.1 Overview
OpenJarvis is an open-source framework developed at Stanford (Hazy Research & Scaling Intelligence Lab) focusing on "Personal AI, On Personal Devices". Its goal is to make local LLMs practical for day-to-day personal assistance, emphasizing energy efficiency ("Intelligence Per Watt"), privacy, and local-first execution.

### 7.2 Core Primitives
1. **Engine Layer (`openjarvis/engine/`)**:
   - Inference abstractions supporting Ollama, LiteLLM, Cloud providers, Apple Foundation Models, Nexa, and NIM.
2. **Agent Layer (`openjarvis/agents/`)**:
   - `OrchestratorAgent`: Tool-calling loop supporting OpenAI function calling and structured `THOUGHT/TOOL/INPUT/FINAL_ANSWER` ReAct loops.
   - `NativeReactAgent`: Classical Thought-Action-Observation agent.
   - `NativeOpenHandsAgent`: CodeAct agent that writes and executes Python code.
   - `MonitorOperative`: Persistent continuous monitoring agent with tick-based scheduling and memory compaction.
   - `DeepResearchAgent`: Multi-hop research agent with citations and web search.
3. **Tool Layer (`openjarvis/tools/`)**:
   - 40+ personal tools: `apple_calendar`, `weather`, `web_search`, `browser`, `file_read`, `file_write`, `shell_exec`, `db_query`, `docker_shell_exec`.
   - Dynamic tool resolution via `tool_resolver.py`.
4. **Memory Layer (`openjarvis/memory/`)**:
   - SQLite-backed conversation store (`store.py`), entity/profile extractor (`extractor.py`), and retrieval service (`service.py`).
5. **Trace System (`openjarvis/traces/`)**:
   - Trace collector (`collector.py`), analyzer (`analyzer.py`), and persistent trace store (`store.py`) recording tokens, latency, cost, and tool calls.
6. **Learning Layer (`openjarvis/learning/`)**:
   - Optimization via DSPy (`optimize/`), routing policy search, and SFT/GRPO training data generation from traces.
7. **Rust Acceleration (`rust/crates/`)**:
   - 17 Rust crates compiled into a single PyO3 cdylib (`openjarvis_rust`) to accelerate Python execution.

---

## 8. AIQA vs OpenJarvis Feature Comparison Matrix

| Capability | OpenJarvis | AIQA | AIQA Status | Worth Adopting? | Reason / Architectural Verdict |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **Agent Abstraction** | Generic personal assistant agents (ReAct, CodeAct) | Specialized QA workflows (Generation, Compilation, Intelligence) | **PARTIALLY EXISTS** | **YES (Natively)** | Adopt the ReAct / multi-turn loop pattern natively in Rust for QA agents (Test Planner, Healing Agent). |
| **Agent Orchestration** | Multi-turn tool loop (`max_turns`), tick-based manager | Deterministic DAG scheduler (`scheduler.rs`, `engine.rs`) | **EXISTS (Different focus)** | **NO** | Keep AIQA's DAG scheduler; it is far superior for coordinating deterministic test suites. |
| **Task Decomposition** | LLM-driven runtime prompt decomposition | Schema-driven test AST compilation (`execution_plan.rs`) | **EXISTS** | **NO** | AIQA's closed AST (`Action`) is safer and avoids hallucinated shell commands. |
| **Model Routing** | Engine-based routing via LiteLLM / Ollama | Enterprise capability-based router (`ai_router.rs`) | **EXISTS (Superior in AIQA)** | **NO** | AIQA's router has explicit capability levels, fallback, and task overrides. Do NOT touch. |
| **Capability Matching** | Basic model string checking | Granular scoring (`reasoning`, `structured_output`, etc.) | **EXISTS (Superior in AIQA)** | **NO** | AIQA already has a much more rigorous scoring model in Rust. |
| **Skill System** | Agentskills.io format, tools as skills | Dual: `SKILL.md` filesystem + SQLite custom skills | **EXISTS** | **PARTIALLY** | Retain AIQA's format; adopt dynamic tool exposure to skills. |
| **Tool Layer** | 40+ personal tools (Calendar, Weather, Bash) | 7 QA test executors (Playwright, Newman, etc.) | **EXISTS (QA-focused)** | **YES (For Agent Reasoning)**| Add an interactive tool-calling interface for agents (e.g. DOM query, schema probe) during generation. |
| **Tool Chaining** | Dynamic agent-driven tool invocation | Fixed DAG task dependencies | **PARTIALLY EXISTS** | **YES (In Agents)** | Allow agents to chain diagnostic tools before finalizing test cases. |
| **Episodic Memory** | Session message store with rolling summaries | SQLite execution records, test cases, findings | **EXISTS** | **YES (Summarization)** | Adopt OpenJarvis's memory compaction/summarization pattern to keep agent prompts bounded. |
| **Semantic Memory** | Vector retrieval over personal notes | Gemini Embeddings + SQLite vector table | **EXISTS** | **NO** | AIQA already implemented Migration 0014 vector store. |
| **Execution Traces** | Fine-grained token, cost, tool-call trace graphs | Realtime event bus + raw evidence files | **PARTIALLY EXISTS** | **YES** | Adopt OpenJarvis trace schema for AI token, cost, and step-level telemetry. |
| **Learning / Tuning** | DSPy prompt optimization, SFT/GRPO training | Past defect/flakiness feedback into prompts | **PARTIALLY EXISTS** | **NO (Phase 0)** | Too complex and resource-heavy for current AIQA needs; defer to future phase. |
| **Continuous Monitoring**| `MonitorOperative` periodic ticks | AIQA scheduler & incomplete run recovery | **EXISTS** | **NO** | AIQA already has robust run recovery and scheduled test executions. |
| **Local Model Support** | Ollama, Apple FM, Nexa | Ollama, MJ Gateway, local endpoints | **EXISTS** | **NO** | Fully supported in `ai_router.rs`. |
| **Policy / Guardrails** | Approval callbacks before dangerous tool execution | Environment-driven `PolicyConfig` + approval gates | **EXISTS** | **NO** | AIQA already enforces enterprise policy and SSRF guards. |

---

## 9. Duplication & Conflict Analysis

Integrating OpenJarvis directly (as code or dependency) would create catastrophic architectural duplication:

1. **AI Routing & Provider Management**:
   - *AIQA*: `ai_router.rs` (4,690 lines of pure, tested Rust with zero external runtime dependencies).
   - *OpenJarvis*: `openjarvis/engine/` (Python LiteLLM wrappers).
   - *Conflict*: Integrating OpenJarvis engines would bypass AIQA's capability scoring, fallback chains, and persisted `ai_settings`.
   - *Authoritative System*: **AIQA `ai_router.rs` MUST remain authoritative.**

2. **Persistence & Database**:
   - *AIQA*: SQLite database managed via SQLx with 14 atomic migrations (`infra/data/orchestrator.sqlite3`).
   - *OpenJarvis*: SQLite database with separate ad-hoc tables (`managed_agents`, `agent_tasks`).
   - *Conflict*: Two competing SQLite databases with conflicting schemas and connection pools.
   - *Authoritative System*: **AIQA `store.rs` MUST remain authoritative.**

3. **Execution Engine vs Agent Executor**:
   - *AIQA*: `engine.rs` (deterministic DAG runner with state machine, cooperative cancellation, and evidence logging).
   - *OpenJarvis*: `openjarvis/agents/executor.py` (Python while-loop calling OpenAI tool functions).
   - *Conflict*: Replacing or running both would destroy AIQA's test execution guarantees, process lifecycle management, and recovery mechanisms.
   - *Authoritative System*: **AIQA `engine.rs` and `executor.rs` MUST remain authoritative.**

4. **Skill Registry**:
   - *AIQA*: `skill_files.rs` (`SKILL.md` reader) + SQLite `skills` table.
   - *OpenJarvis*: `openjarvis/skills/` (Python parser for agentskills.io).
   - *Conflict*: Duplicate parser and file resolution paths.
   - *Authoritative System*: **AIQA `skill_files.rs` MUST remain authoritative.**

---

## 10. Rust vs Python Integration Analysis

| Approach | Latency | Reliability | Deployment | Maintenance | Verdict |
| :--- | :--- | :--- | :--- | :--- | :--- |
| **A. Direct Dependency (`cargo add openjarvis`)** | N/A | Impossible | Impossible | Impossible | **REJECTED**: OpenJarvis is a Python package; cannot be linked into Rust directly. |
| **B. Python Subprocess / Child Process** | High (>300ms IPC) | Low (child process crashes, zombie processes) | Terrible (requires system Python, venv, uv, pip) | High | **REJECTED**: Fragile runtime dependencies; violates AIQA's self-contained binary architecture. |
| **C. Separate Microservice (HTTP/gRPC)** | Moderate (10-50ms network) | Moderate (network partitions, separate failure domains) | Complex (two services to deploy, monitor, configure) | High | **REJECTED**: Unnecessary operational burden for an application designed to run on a single machine. |
| **D. Shared Architectural Concepts Only** | Zero | High | Ideal (single binary) | Low | **VIABLE**: Understand the patterns, implement cleanly. |
| **E. Native Rust Implementation of Selected Concepts** | Ultra-low (zero IPC) | Highest (type safety, compiler checks) | Clean (single `aiqa-orchestrator.exe` binary) | Lowest | **RECOMMENDED**: Implement QA Agent abstraction and ReAct loop natively in Rust. |

---

## 11. Proposed AIQA Native Hybrid Architecture

Without modifying any code in Phase 0, here is the validated design for future phases:

```text
========================================================================================
                                     AIQA PLATFORM
========================================================================================

                         +-------------------------------+
                         |      React 19 Web UI          |
                         +---------------+---------------+
                                         | HTTP / WebSocket
                                         v
+--------------------------------------------------------------------------------------+
|                                 RUST AXUM BACKEND                                    |
|                                                                                      |
|  +--------------------------------------------------------------------------------+  |
|  |                           AIQA ORCHESTRATOR CORE                               |  |
|  |                                                                                |  |
|  |   +-------------------------------------------------------------------------+  |  |
|  |   |             QA AGENT LAYER (New Concept from OpenJarvis)                |  |  |
|  |   |  - Native Rust ReAct Loop (Thought -> Tool -> Observation -> Result)    |  |  |
|  |   |  - Agent Types:                                                         |  |  |
|  |   |    * Test Planning Agent (Decomposes complex apps into test suites)     |  |  |
|  |   |    * Grounded Generation Agent (Wave-based generation with reasoning)   |  |  |
|  |   |    * Self-Healing Agent (Analyzes failed execution & adapts AST)        |  |  |
|  |   |    * Bug Triage Agent (Deep root-cause hypothesis generation)           |  |  |
|  |   +------------------------------------+------------------------------------+  |  |
|  |                                        |                                          |  |
|  |         +------------------------------+------------------------------+           |  |
|  |         v                                                             v           |  |
|  |  +---------------+  (Calls during reasoning)   +------------------------------+   |  |
|  |  | QA Tool Layer | <-------------------------- | Central AI Infrastructure    |   |  |
|  |  | - Inspect DOM |                             | (ai_router.rs - UNCHANGED)   |   |  |
|  |  | - Probe Schema|                             | - Provider Discovery         |   |  |
|  |  | - Dry-run Step|                             | - Capability Scoring         |   |  |
|  |  +-------+-------+                             | - Model Fallback Chain       |   |  |
|  |          |                                     +------------------------------+   |  |
|  |          v                                                                        |  |
|  |  +--------------------------------------------------------------------------+     |  |
|  |  | Test Execution Compiler (execution_plan.rs - PRESERVED)                  |     |  |
|  |  | - Compiles Agent outputs to closed Action AST                            |     |  |
|  |  +------------------------------------+-------------------------------------+     |  |
|  |                                       v                                           |  |
|  |  +--------------------------------------------------------------------------+     |  |
|  |  | DAG Scheduler & Engine (engine.rs, scheduler.rs - PRESERVED)             |     |  |
|  |  | - Run State Machine, Task Dispatching, Cancellation, Recovery            |     |  |
|  |  +------------------------------------+-------------------------------------+     |  |
|  |                                       v                                           |  |
|  |  +--------------------------------------------------------------------------+     |  |
|  |  | Executor Registry (executor.rs - PRESERVED)                               |     |  |
|  |  | - Playwright | Newman | Pytest | Maestro | Schemathesis | CompiledApi     |     |  |
|  |  +--------------------------------------------------------------------------+     |  |
|  +--------------------------------------------------------------------------------+  |
|                                                                                      |
|  +--------------------------------------------------------------------------------+  |
|  |                        CROSS-CUTTING PLATFORM SERVICES                         |  |
|  |  - Policy & Guardrails (policy.rs)             - Quality Intelligence (intel.rs)|  |
|  |  - Evidence Store (evidence.rs)                - SQLx SQLite Store (store.rs)   |  |
|  |  - Auth & Governance (auth.rs)                 - Step/Token Traces (New Trace)  |  |
|  +--------------------------------------------------------------------------------+  |
+--------------------------------------------------------------------------------------+
```

---

## 12. Components to Preserve, Add, and NOT Add

### 12.1 Components to PRESERVE (DO NOT TOUCH)
- **AI Router (`ai_router.rs`)**: Exceptional implementation; far more rigorous capability scoring and provider management than OpenJarvis.
- **Executor Registry (`executor.rs`)**: Complete, secure process isolation with Windows job handling, timeouts, and SSRF guards.
- **Scheduler & State Machine (`engine.rs`, `scheduler.rs`)**: Battle-tested DAG scheduling and crash-recovery logic.
- **Execution Plan AST (`execution_plan.rs`)**: Closed, safe action enum (`Navigate`, `Click`, `Request`, etc.) preventing arbitrary code execution.
- **Evidence Store (`evidence.rs`)**: File-backed SHA-256 evidence integrity store.
- **SQLite Database (`store.rs`)**: Clean, fully-migrated relational schema.
- **Existing Skills (`skill_files.rs`)**: Fast, dynamic filesystem scanner.

### 12.2 Components to ADD Natively in Rust (Future Phases)
1. **QA Agent Contract & ReAct Loop**: A generic multi-turn reasoning trait in Rust (`Thought -> Tool Call -> Observation -> Result`) allowing agents to solve complex QA tasks iteratively.
2. **QA Agent Tool Protocol**: Exposing read-only diagnostic tools (schema inspection, DOM selector verification, API endpoint probing) to the agent during generation.
3. **Structured Step & Token Trace System**: Capturing LLM token counts, latency, and tool-call steps per request for granular observability.
4. **Memory Compaction / Rolling Summaries**: Bounding context windows by automatically summarizing prior observations in long-horizon test workflows.

### 12.3 Components to NOT Add
- **Generic Personal Assistant Tools**: Weather, calendar, Spotify, generic bash execution, desktop GUI automation.
- **Python / PyO3 Runtime**: No Python interpreters, no virtual environments, no uv lockfile dependencies inside AIQA.
- **OpenJarvis Engine Layer**: Do not replace or supplement `ai_router.rs` with LiteLLM or OpenJarvis engine abstractions.
- **DSPy / SFT / GRPO Training Pipelines**: Overly complex, high compute requirements, out of scope for AIQA's core mission.

---

## 13. Risk Analysis

| Risk | Severity | Mitigation Strategy |
| :--- | :--- | :--- |
| **Architectural Bloat** | High | Strictly reject Python dependencies; implement only minimal, clean Rust traits. |
| **Hallucinated Execution** | High | Maintain AIQA's closed AST (`Action`); never give agents arbitrary shell or Python execution capabilities. |
| **Context Window Exhaustion** | Medium | Implement OpenJarvis-style memory compaction and rolling summaries in multi-turn agent loops. |
| **Performance Degradation** | Medium | Ensure agent tool calls are lightweight and timeout-bounded; preserve single-call paths for simple test generation. |
| **Breaking Existing Tests** | High | All existing endpoints and generation workflows must remain default; new agent workflows will be strictly opt-in. |

---

## 14. Phased Implementation Roadmap (For Future Phases)

> [!IMPORTANT]
> **DO NOT IMPLEMENT ANY PHASE YET.** This plan is strictly for review and future planning.

### Phase 1: Native Agent Abstraction & Tool Contract
- **Objective**: Define native Rust traits for multi-turn agents and interactive QA tools.
- **Affected Modules**: New `services/orchestrator/src/agent.rs`, `services/orchestrator/src/tools.rs`.
- **Risk**: Very Low (additive traits, no existing code modified).
- **Acceptance Criteria**: Unit tests verifying an agent can loop through mock tools and terminate on final answer.

### Phase 2: QA Diagnostic Tools
- **Objective**: Implement read-only tools for agents (Schema Inspector, DOM Prober, Endpoint Validator).
- **Affected Modules**: `tools.rs`, wired to `crawler.rs` and `doc_extract.rs`.
- **Risk**: Low (read-only operations with SSRF guards).
- **Acceptance Criteria**: Tools execute safely and return structured JSON observations.

### Phase 3: Specialized QA Agents
- **Objective**: Implement concrete QA agents: Test Planning Agent and Self-Healing Agent.
- **Affected Modules**: `services/orchestrator/src/agents/planning.rs`, `healing.rs`.
- **Risk**: Medium (prompts require tuning for high reliability).
- **Acceptance Criteria**: Self-healing agent correctly diagnoses a broken selector and updates the test plan.

### Phase 4: Observability & Step Tracing
- **Objective**: Structured logging of agent turns, token usage, and latency.
- **Affected Modules**: `log_bus.rs`, new `traces.rs`, migration for trace storage.
- **Risk**: Low.
- **Acceptance Criteria**: Traces queryable via REST API and displayed on frontend execution hub.

---

## 15. Final Decision Matrix

| Question | Decision | Explanation |
| :--- | :--- | :--- |
| **1. Should OpenJarvis be directly integrated into AIQA?** | **NO** | Divergent domains (personal assistant vs QA platform); would create catastrophic architecture conflict. |
| **2. Should OpenJarvis be installed as an AIQA dependency?** | **NO** | OpenJarvis is Python-based; would introduce heavy external interpreter, venv, and package bloat. |
| **3. Should OpenJarvis be run as a separate service?** | **NO** | Adds zero value over native implementation; adds IPC latency and dual-deployment complexity. |
| **4. Should OpenJarvis architecture patterns be adopted natively?** | **YES** | The ReAct multi-turn loop, agent tool interface, and token tracing are high-value concepts. |
| **5. Which exact concepts should be adopted?** | — | 1) Multi-turn ReAct agent loop<br>2) Interactive QA tool layer<br>3) Step & token trace logging<br>4) Context memory compaction |
| **6. Which concepts should NOT be adopted?** | — | 1) Personal assistant tools (calendar, weather)<br>2) Arbitrary shell execution<br>3) Python engine abstraction<br>4) SFT/GRPO training pipelines |
| **7. What is the highest-priority missing capability in AIQA?** | — | **Interactive multi-turn tool calling during AI reasoning** (AI cannot currently probe or dry-run steps before outputting test cases). |
| **8. What should be implemented first?** | — | **Phase 1: Native Rust `Agent` and `AgentTool` traits** built cleanly on top of `ai_router::AiInfrastructure`. |

---

## 16. Exact Recommended Next Step

**Action**: Await User Approval of this Phase 0 Architecture Audit Report.
**Next Phase**: Once approved, formulate the design specification for **Phase 1: Native Rust Agent Abstraction & Tool Contract**.
