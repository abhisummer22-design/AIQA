"""Python-side mirror of the orchestrator's Executor contract.

Field names and the status vocabulary here must stay in sync with
`services/orchestrator/src/domain.rs` (TaskDef, TaskStatus) and
`services/orchestrator/src/executor.rs` (Executor, ExecutionOutcome) — those
Rust definitions are authoritative; this module exists so a future Python
worker speaks the same shapes over the wire (SRD §16: "All use a common API
Executor contract" / SRD §17: "Each executor declares capabilities and
safety requirements").
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import Enum


class TaskStatus(str, Enum):
    """Mirrors FRD §2 Test Status Model. Only terminal variants are valid
    as an ExecutionOutcome.status — QUEUED/RUNNING are orchestrator-owned."""

    PASSED = "PASSED"
    FAILED = "FAILED"
    TIMEOUT = "TIMEOUT"
    INFRA_ERROR = "INFRA_ERROR"
    NEEDS_REVIEW = "NEEDS_REVIEW"


@dataclass
class TaskDef:
    id: str
    name: str
    prerequisites: list[str] = field(default_factory=list)
    # Adapter-specific execution parameters (collection path, spec file, flow
    # name, ...) — shape is per-executor and intentionally untyped here.
    params: dict = field(default_factory=dict)


@dataclass
class ExecutionOutcome:
    status: TaskStatus
    reason: str | None = None
    # Evidence object keys already uploaded to object storage for this task
    # (SRD §15 Evidence Architecture) — empty until a real adapter exists.
    evidence_refs: list[str] = field(default_factory=list)


class Executor(ABC):
    """One implementation per real tool (Playwright, Newman/HTTPX/Schemathesis,
    Maestro/Appium, Strix). See `adapters/` for the current (unimplemented)
    stand-ins and SRD §16-17 for the capability/safety requirements each
    concrete adapter must declare.
    """

    #: Machine-readable capability tag reported to the orchestrator, e.g.
    #: "web", "mobile", "api", "security" (SRD §17).
    kind: str

    @abstractmethod
    async def execute(self, task: TaskDef) -> ExecutionOutcome:
        raise NotImplementedError
