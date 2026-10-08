-- 0029_qa_releases.sql
-- Phase 2 (QA Intelligence Memory): a first-class release/build identity.
--
-- Why this migration exists:
--
-- Release Risk is an entire phase of the master plan, and there was nothing for
-- it to read. The audit (docs/qa-intelligence-memory/PHASE_1_AUDIT.md §1.1)
-- found zero tables, zero columns and zero source references for a release,
-- build, version or commit identity. Nothing else can substitute:
--   - `regression_schedules` is a *timer*, not a release
--   - `runs.plan` carries execution structure, not what was shipped
--   - `audit_log` records who did what, not what was built
--
-- This is the one genuinely new entity in the whole plan, because unlike module
-- (which can be a `quality_nodes` node type on the existing graph) a release has
-- no equivalent representation anywhere in the schema.
--
-- Deliberate design choices, each of which stops this from fabricating history:
--
-- 1. `runs.release_id` is NULLABLE and is never auto-populated. A run that
--    nobody associated with a release simply has none, and release intelligence
--    answers INSUFFICIENT_DATA rather than inventing an association.
-- 2. Nothing backfills. Existing runs stay NULL.
-- 3. No commit-sha / build-number column is required. `label` and `version` are
--    both optional because a team may track one, the other, or neither; the
--    stable identity is the UUID.
-- 4. UNIQUE (project_id, label) where label is present. SQLite treats NULLs as
--    distinct, so two NULL labels do not collide - which is the intent, since an
--    unlabelled release is still its own release.
--
-- Safety: a new table plus a nullable column. No existing data is touched.

CREATE TABLE IF NOT EXISTS qa_releases (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    label TEXT,
    version TEXT,
    notes TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL,
    released_at TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_releases_project_label
    ON qa_releases(project_id, label);

-- Serves `list_releases` (project-scoped, newest first) which is the only read
-- path Phase 5 needs.
CREATE INDEX IF NOT EXISTS idx_releases_project_created
    ON qa_releases(project_id, created_at DESC);

ALTER TABLE runs ADD COLUMN release_id TEXT;

-- Serves "every run of this release", the historical comparison Phase 5 performs.
CREATE INDEX IF NOT EXISTS idx_runs_release ON runs(release_id);