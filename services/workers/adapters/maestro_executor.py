"""Mobile executor adapter (SRD §17: "Maestro handles primary mobile flows").

Not implemented. When built, this should acquire an exclusive device lock
(SRD §7 Parallelism & Resource Locks — "exclusive mobile devices") before
running a Maestro flow, and release it in a `finally` block regardless of
outcome.
"""

from __future__ import annotations

from executor_contract import Executor, ExecutionOutcome, TaskDef


class MaestroExecutor(Executor):
    kind = "mobile"

    async def execute(self, task: TaskDef) -> ExecutionOutcome:
        raise NotImplementedError(
            "MaestroExecutor is a stub — see services/workers/README.md"
        )
