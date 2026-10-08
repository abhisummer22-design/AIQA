-- Phase 3 (Change Impact Intelligence) — scope `change_impacts` and make duplicate
-- records impossible.
--
-- WHY THIS EXISTS
--
-- `change_impacts` has been real schema since migration 0002 and has had **zero writers**
-- for its entire life. Phase 3 implemented the first writer, and doing so exposed two
-- defects in the table that a 0-row table had been hiding:
--
-- 1. NO PROJECT SCOPE. The table had no `project_id` column. The only path to a tenant
--    was `affected_node_id -> quality_nodes.project_id`, so every read had to join through
--    the graph to establish ownership. That works, but it means the one table that records
--    "what did this change break" was the only table in the schema where a forgotten
--    `WHERE` is a cross-tenant leak rather than an empty result. Added as a column so
--    scope is stated, not inferred.
--
-- 2. NO UNIQUE KEY. Re-computing impact for the same change would insert a duplicate row
--    every time, so repeated analysis would inflate the record of a single change rather
--    than confirm it. Added as a unique index so a repeated computation is idempotent.
--
-- BOTH CHANGES ARE ADDITIVE.
--
-- * `ADD COLUMN` on SQLite leaves existing rows intact, giving them NULL. There are no
--   rows in any deployed database this migration has run against, but NULL is tolerated
--   deliberately rather than assumed: a pre-migration row is distinguishable, and the
--   writer never produces one.
-- * A unique index over a table with no conflicting rows cannot fail.
--
-- NOTHING IS DROPPED OR REWRITTEN. No table is recreated, so this is not a destructive
-- migration and does not trip the plan's stop conditions.
--
-- WHY THE UNIQUE INDEX USES COALESCE
--
-- `selected_test_node_id` is nullable — it is NULL when a change is recorded as affecting
-- a node without any test being selected for it, which is a real and useful negative
-- result. SQLite treats NULLs as distinct in a unique index, so a plain index on the raw
-- column would let unlimited NULL-bearing duplicates through. `COALESCE(..., '')` folds
-- NULL to a single value so those rows are covered too. Expression indexes have been
-- supported by SQLite since 3.9 (2015), and the project already requires a newer engine
-- than that for its `ON CONFLICT` upserts.

ALTER TABLE change_impacts ADD COLUMN project_id TEXT;

-- Reading a project's change history, newest first. Covers the common query without a
-- scan once a project has accumulated impact records.
CREATE INDEX IF NOT EXISTS idx_change_impacts_project
    ON change_impacts(project_id, computed_at DESC);

-- One record per (change, affected node, selected test). Makes re-computing the same
-- impact idempotent rather than additive.
CREATE UNIQUE INDEX IF NOT EXISTS uq_change_impacts_record
    ON change_impacts(
        project_id,
        change_ref,
        affected_node_id,
        COALESCE(selected_test_node_id, '')
    );