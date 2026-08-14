"""API executor adapter (SRD §16: "Newman runs exported Postman collections
in a dedicated Node worker" — note this one is Node-based in the target
architecture, unlike HTTPX/Pytest and Schemathesis which are native Python).

This file stands in for the orchestration-side wrapper that would shell out
to (or otherwise invoke) the Node/Newman worker and translate its report into
`ExecutionOutcome`. Not implemented.
"""

from __future__ import annotations

from executor_contract import Executor, ExecutionOutcome, TaskDef


class NewmanExecutor(Executor):
    kind = "api"

    async def execute(self, task: TaskDef) -> ExecutionOutcome:
        raise NotImplementedError(
            "NewmanExecutor is a stub — see services/workers/README.md"
        )
