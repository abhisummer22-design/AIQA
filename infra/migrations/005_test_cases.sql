-- Target-schema mirror of services/orchestrator/migrations/0005_test_cases.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale). Phase 3 gap-closing. `suite_id` is intentionally NOT
-- included here — it is added later, by 011_test_plan_suite_hierarchy.sql,
-- in the same order the real schema actually acquired it (test_suites does
-- not exist yet at this point).
--
-- Real, persisted, project-isolated Test Case entity. `project_id` is UUID
-- but carries no FK, same reasoning as 004_projects.sql's own comment.
-- `preconditions`/`steps`/`assertions`/`tags`/`traceability` are JSONB here
-- (SQLite stores them as opaque JSON text because it has no native JSON
-- column type — Postgres does, so this is a real, direct upgrade, not a
-- structural change: the backend still stores/returns them without
-- interpreting their internal shape at the SQL layer either way).
--
-- Lifecycle (`status`): GENERATED -> VALIDATED -> APPROVED -> EXECUTED,
-- with REJECTED reachable from GENERATED or VALIDATED. Kept as TEXT rather
-- than a Postgres ENUM, matching this schema's own established convention
-- (001/002 reserve real ENUM types for `run_state`/`task_status` only;
-- every other status-shaped column — `review_state`, `node_type`,
-- `relation_type`, etc. — is TEXT there too).
CREATE TABLE test_cases (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id              UUID NOT NULL,
    title                   TEXT NOT NULL,
    description             TEXT NOT NULL,
    category                TEXT NOT NULL,
    priority                TEXT NOT NULL,
    severity                TEXT NOT NULL,
    preconditions           JSONB NOT NULL,
    steps                   JSONB NOT NULL, -- array of {step_number, action, expected_result}
    expected_result         TEXT NOT NULL,
    assertions              JSONB NOT NULL,
    test_data               TEXT,
    source_document         TEXT,
    requirement_reference   TEXT,
    source_requirement      TEXT,
    traceability            JSONB, -- opaque, backend does not interpret
    risk_factor             TEXT,
    tags                    JSONB NOT NULL DEFAULT '[]',
    source                  TEXT NOT NULL, -- how this case was produced (e.g. "generator")
    status                  TEXT NOT NULL DEFAULT 'GENERATED',
    validation_notes        TEXT,
    executed_run_id         UUID,
    executed_task_id        TEXT, -- plan-scoped id, not necessarily a UUID (see tasks.id in 001)
    created_by              TEXT,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at              TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_test_cases_project ON test_cases (project_id, created_at);
CREATE INDEX idx_test_cases_project_status ON test_cases (project_id, status);
