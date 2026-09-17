-- Target-schema mirror of services/orchestrator/migrations/0011_test_plan_suite_hierarchy.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale). Phase 3 gap-closing.
--
-- A real plan -> suite -> case hierarchy above the flat `test_cases` table
-- (005). Both new tables are OPTIONAL containers: `test_suites.plan_id` and
-- `test_cases.suite_id` are nullable, so every test case predating this
-- migration (or created afterward without ever being assigned to a suite)
-- remains fully valid with no backfill required — same "don't retrofit
-- structure onto data that predates it" principle 004_projects.sql's own
-- comment already established for `project_id`.
--
-- `project_id` on both new tables is UUID with no FK, matching every other
-- project-scoped table in this schema (see 004_projects.sql's own comment).
--
-- `plan_id`/`suite_id` ARE real FKs, `ON DELETE SET NULL` (not CASCADE) —
-- matching the real SQLite schema's own foreign-key topology exactly:
-- deleting a plan or suite detaches its contents rather than destroying
-- them, since a real, persisted test case is valuable, standalone-able data.
CREATE TABLE test_plans (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id      UUID NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT,
    status          TEXT NOT NULL DEFAULT 'DRAFT', -- DRAFT | ACTIVE | ARCHIVED
    entry_criteria  TEXT,
    exit_criteria   TEXT,
    created_by      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_test_plans_project ON test_plans (project_id, created_at);

CREATE TABLE test_suites (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id      UUID NOT NULL,
    plan_id         UUID REFERENCES test_plans(id) ON DELETE SET NULL,
    name            TEXT NOT NULL,
    description     TEXT,
    created_by      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL
);
CREATE INDEX idx_test_suites_project ON test_suites (project_id, created_at);
CREATE INDEX idx_test_suites_plan ON test_suites (plan_id);

ALTER TABLE test_cases ADD COLUMN suite_id UUID REFERENCES test_suites(id) ON DELETE SET NULL;
CREATE INDEX idx_test_cases_suite ON test_cases (suite_id);
