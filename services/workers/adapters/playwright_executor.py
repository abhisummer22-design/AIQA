"""Web executor adapter (SRD §17: "Playwright handles web").

Not implemented. When built, this should drive Playwright against
`task.params` (target URL/flow), capture screenshots/video/trace as evidence
(SRD §15, §18), and map the Playwright result onto `ExecutionOutcome`.
"""

from __future__ import annotations

from executor_contract import Executor, ExecutionOutcome, TaskDef


class PlaywrightExecutor(Executor):
    kind = "web"

    async def execute(self, task: TaskDef) -> ExecutionOutcome:
        raise NotImplementedError(
            "PlaywrightExecutor is a stub — see services/workers/README.md"
        )
