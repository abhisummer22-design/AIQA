# AIQA Workers (Python) — stub

Placeholder for the Python AI/QA worker fleet described in SRD §1 ("Python AI/QA workers") and
§16-17 (API/Web/Mobile/Security executor adapters). Nothing here talks to the orchestrator yet —
this only pins down the shared contract real adapters will implement, so the Rust orchestrator's
`Executor` trait (`services/orchestrator/src/executor.rs`) has a matching counterpart on this side
once workers exist.

## Why this is a stub, not an implementation

Building real executors (Playwright, Newman/HTTPX/Schemathesis, Maestro/Appium, Strix) requires
resolving open product questions first (see the PRD critique from planning): the MVP boundary,
whether AI planning runs on local (Ollama) or hosted models, and how a worker authenticates to the
Rust control plane. Shipping adapter code ahead of those decisions would just get rewritten.

## Contract

`executor_contract.py` defines the Python-side mirror of the orchestrator's task/outcome shapes
(`TaskDef`, `ExecutionOutcome`) and an `Executor` ABC. `adapters/` has one file per target executor
(`playwright_executor.py`, `newman_executor.py`, `maestro_executor.py`) — each currently just
raises `NotImplementedError` with a docstring pointing at the relevant SRD section, marking where
real work starts.

## When this becomes real

A worker will need to: register with the orchestrator (or poll/subscribe for assigned tasks over
Redis per SRD §1 "Redis task coordination"), run the real tool, translate its result into
`ExecutionOutcome`, and report back over the same event schema `services/orchestrator/src/domain.rs`
defines (`RunEvent` — event_id/run_id/task_id/sequence/timestamp/schema_version). None of that
transport exists yet; today the orchestrator only knows how to run its in-process
`SimulatedExecutor`.
