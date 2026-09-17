-- Target-schema mirror of services/orchestrator/migrations/0004_projects.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale, reproduced in brief below). Phase 3 gap-closing.

-- Adds ONLY the `projects` table itself. Deliberately does NOT add a foreign
-- key from the existing `project_id` columns on `runs`/`api_keys`/
-- `quality_nodes`/`failure_clusters`/`audit_log`, and does NOT backfill a
-- `projects` row for their pre-existing values: those columns are free-text
-- scoping keys with values that predate any project entity, matching the
-- exact same disclosed reasoning the real SQLite migration gives (this is a
-- genuine design decision carried over unchanged, not a SQLite-specific
-- limitation — a fresh Postgres deployment would face the identical
-- "retrofitting a FK forces a backfill for untenanted/free-text data" issue
-- the moment any pre-existing row exists).
CREATE TABLE projects (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    description TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL
);
