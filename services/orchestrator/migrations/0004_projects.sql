-- Phase 2 / BE-8 — the `projects` entity (API Contract §5.1-5.4).
--
-- Scope note: this migration adds ONLY the new `projects` table itself. It
-- deliberately does NOT add a foreign key from the existing `project_id`
-- columns on `runs`/`api_keys`/`quality_nodes`/`failure_clusters`/
-- `audit_log`, and does NOT backfill a `projects` row for their existing
-- free-text values ("alpha", "beta", "default", etc.).
--
-- Why deferred rather than done now: those columns are live, Phase-1-hardened
-- data with values that predate any project entity. Retrofitting a FK onto
-- them requires a backfill migration that invents a `projects` row per
-- distinct existing string (including for untenanted/NULL runs) and touches
-- tables outside this unit's stated scope (BE-8 = the Projects API itself).
-- `AIQA_BACKEND_IMPLEMENTATION_PLAN_v1.0.md` §8 lists that as its own
-- migration (`0005_project_denormalisation`) for exactly this reason. Doing
-- it here would be schema-affecting production risk ahead of the unit that
-- actually consumes it — see `AIQA_PHASE2_BE8_IMPLEMENTATION_REPORT.md` §1
-- for the full reasoning.
--
-- Existing `project_id` scoping (`require_project_access` in `auth.rs`,
-- already verified in Phase 1) is completely unaffected: it compares a
-- caller's key's `project_id` string against a resource's `project_id`
-- string, independent of whether a `projects` row exists for that string.

CREATE TABLE projects (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
