-- AUD-005 — real, persisted, project-isolated Test Case entity, closing the
-- Test Case Generator -> Persistence -> Validation -> Execution Hub gap
-- identified in AIQA_FRESH_FULL_PROJECT_AUDIT_REPORT.md.
--
-- Field set derived from FRD-VAL-05 ("Each case retains category, priority,
-- preconditions, steps, expected result and traceability") plus this
-- backend's existing conventions (project_id as a free-text column, no FK —
-- same as every other project-scoped table since 0004_projects.sql; see that
-- migration's own comment for why). No fields were added beyond what
-- FRD-VAL-05 and the existing generator's own real output already require.
--
-- Lifecycle (`status`), grounded directly in FRD-EXE-11 ("Convert approved
-- test cases into schedulable tasks" — only APPROVED cases convert) and
-- FRD-VAL-02 (generated cases must meet quality rules before that):
--   GENERATED -> VALIDATED -> APPROVED -> EXECUTED
--                     \-> REJECTED (from GENERATED or VALIDATED)
-- "EXECUTABLE" as a distinct state was considered and dropped as redundant
-- with APPROVED (FRD-EXE-11 already ties "approved" directly to schedulable);
-- no state was added beyond what the documented flow requires.
--
-- `executed_run_id`/`executed_task_id` are the real execution-traceability
-- link (Test Case -> Run -> Task -> Result -> Evidence -> Findings): set once,
-- at handoff, to the real `runs.id`/`tasks.id` the generator's own handoff
-- created. No new join table needed — `runs`/`tasks`/`evidence`/`run_events`
-- are already queryable by these two ids via their own existing tables.

-- Column set matches `domain.rs::CreateTestCaseRequest` one-to-one (already
-- present in the working tree at the time this migration was authored — see
-- that struct's own doc comment). `traceability` stays opaque JSON text, the
-- same convention this schema already uses for `risk_assessments.inputs`/
-- `quality_nodes` metadata-shaped columns elsewhere: the backend stores and
-- returns it without interpreting its internal shape.
CREATE TABLE test_cases (
    id                      TEXT PRIMARY KEY,
    project_id              TEXT NOT NULL,
    title                   TEXT NOT NULL,
    description             TEXT NOT NULL,
    category                TEXT NOT NULL,
    priority                TEXT NOT NULL,
    severity                TEXT NOT NULL,
    preconditions           TEXT NOT NULL, -- JSON array of strings
    steps                   TEXT NOT NULL, -- JSON array of {step_number, action, expected_result}
    expected_result         TEXT NOT NULL,
    assertions              TEXT NOT NULL, -- JSON array of strings
    test_data               TEXT,
    source_document         TEXT,
    requirement_reference   TEXT,
    source_requirement      TEXT,
    traceability            TEXT, -- opaque JSON, backend does not interpret
    risk_factor             TEXT,
    tags                    TEXT NOT NULL DEFAULT '[]', -- JSON array of strings
    source                  TEXT NOT NULL, -- how this case was produced (e.g. "generator")
    status                  TEXT NOT NULL DEFAULT 'GENERATED',
    validation_notes        TEXT,
    executed_run_id         TEXT,
    executed_task_id        TEXT,
    created_by              TEXT,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL
);

CREATE INDEX idx_test_cases_project ON test_cases (project_id, created_at);
CREATE INDEX idx_test_cases_project_status ON test_cases (project_id, status);
