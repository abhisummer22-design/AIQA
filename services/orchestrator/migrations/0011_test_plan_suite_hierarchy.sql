-- Phase 5 (Output maturity) — a real plan -> suite -> case hierarchy above
-- the existing flat `test_cases` table (0005_test_cases.sql). Both new
-- tables are OPTIONAL containers: `test_suites.plan_id` and
-- `test_cases.suite_id` are nullable, so every existing test case (created
-- before this migration, or created afterward without ever being assigned
-- to a suite) remains fully valid with no backfill required — the same
-- "don't retrofit structure onto data that predates it" principle
-- 0004_projects.sql's own comment already established for `project_id`.
--
-- `project_id` on both new tables is free-text, no FK to `projects` —
-- matching every other project-scoped table in this schema (see
-- 0004_projects.sql's own comment for why a real FK was deliberately not
-- retrofitted onto existing project_id columns).
--
-- `plan_id`/`suite_id` ARE real FKs, `ON DELETE SET NULL` (not CASCADE):
-- deleting a plan or suite detaches its contents rather than destroying
-- them — a real, persisted test case is valuable, standalone-able data;
-- removing its container must never silently delete what was inside it.
-- (`Store::connect` already runs with `foreign_keys = ON` — see that
-- function's own comment.)

CREATE TABLE test_plans (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL,
    name            TEXT NOT NULL,
    description     TEXT,
    status          TEXT NOT NULL DEFAULT 'DRAFT', -- DRAFT | ACTIVE | ARCHIVED
    entry_criteria  TEXT,
    exit_criteria   TEXT,
    created_by      TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX idx_test_plans_project ON test_plans (project_id, created_at);

CREATE TABLE test_suites (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL,
    plan_id         TEXT REFERENCES test_plans(id) ON DELETE SET NULL,
    name            TEXT NOT NULL,
    description     TEXT,
    created_by      TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX idx_test_suites_project ON test_suites (project_id, created_at);
CREATE INDEX idx_test_suites_plan ON test_suites (plan_id);

ALTER TABLE test_cases ADD COLUMN suite_id TEXT REFERENCES test_suites(id) ON DELETE SET NULL;
CREATE INDEX idx_test_cases_suite ON test_cases (suite_id);
