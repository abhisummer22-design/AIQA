-- Indexes for the project-scoped reads the dashboard, the KPI tiles and the
-- run/evidence/finding lists all poll on every load.
--
-- Why this migration exists: every one of these indexes was missing, which was
-- proven against the live schema with `EXPLAIN QUERY PLAN` rather than assumed
-- from a slow dashboard. The recorded plans were a bare `SCAN runs` plus a
-- temporary B-tree, i.e. a full table scan served on a hot endpoint.
--
-- 1. `runs(project_id, created_at)` — `runs` carried no index at all beyond the
--    primary key on `id` (`sqlite_autoindex_runs_1`). Every project-scoped run
--    query therefore scanned the whole table:
--      store.rs::list_run_ids              (ORDER BY created_at DESC)
--      store.rs::dashboard_run_counts      (GROUP BY state)
--      store.rs::dashboard_avg_run_duration_ms
--    Leading column `project_id` serves all three; `created_at` is included so
--    the `ORDER BY created_at` also stops being a temporary B-tree sort. This
--    single index is the cheapest high-value change in the schema.
--
--    It also removes a real contention path: `store.rs::delete_project` opens
--    `BEGIN IMMEDIATE` and then runs 20+ sequential `DELETE`s, one of which is
--    `DELETE FROM runs WHERE project_id = ?`. Unindexed, that was a full scan
--    performed while holding SQLite's single global write lock, so the whole
--    window blocked every other writer (task status, run events, telemetry,
--    audit) on the busy handler.
--
-- 2. `tasks(run_id, status)` — `dashboard_task_counts` joins tasks to runs and
--    groups by `tasks.status`. The implicit `(run_id, id)` primary-key index
--    already covers the join, but the status grouping did a full read. This
--    supports the count without materialising the group set. The query is
--    rewritten (see `dashboard_task_counts`) to drive from `runs` instead, so
--    the project filter is what narrows the scan; this index then serves the
--    per-run status lookup.
--
-- 3. `bug_drafts(failure_cluster_id, created_at DESC)` — `bug_drafts` had NO
--    index at all beyond its primary key. The findings list is polled and joins
--    `bug_drafts` to `failure_clusters` ordered by `created_at DESC`, which
--    planned as `SCAN d` + a temporary B-tree. That endpoint also had no
--    `LIMIT`, so it returned every draft ever created for a project.
--
-- 4. `audit_log(action, occurred_at)` — the Prometheus collector in
--    `metrics.rs` counts `audit_log` rows by `action`, and does so twice per
--    scrape, on a table that is append-only and never pruned. Only
--    `idx_audit_log_occurred_at` existed (migration 0003), so both counts were
--    full scans that grow without bound.
--
-- Indexes are additive and idempotent to apply: `IF NOT EXISTS` keeps this safe
-- to re-run, and none of them change a query's result — only how it is found.

CREATE INDEX IF NOT EXISTS idx_runs_project_created
    ON runs (project_id, created_at);

CREATE INDEX IF NOT EXISTS idx_tasks_run_status
    ON tasks (run_id, status);

CREATE INDEX IF NOT EXISTS idx_bug_drafts_cluster_created
    ON bug_drafts (failure_cluster_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_audit_log_action_occurred
    ON audit_log (action, occurred_at);