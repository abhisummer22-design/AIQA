-- 0028_qa_memory_linkage.sql
-- Phase 2 (QA Intelligence Memory): make the test-case <-> execution link durable.
--
-- Why this migration exists:
--
-- `execution_telemetry` already carries a `test_case_id` column (added by
-- 0016_adaptive_intelligence.sql), and it is NULL in every row the system has
-- ever written. The reason is that the forward link had nowhere to come from:
-- `tasks` had no `test_case_id` at all, so the only place a case id existed was
-- inside the `runs.plan` JSON blob - unqueryable, unindexed and lost the moment
-- anything read the plan.
--
-- The consequence is that every per-test-case question the product wants to ask
-- about history ("how stable is this case", "has this case failed before",
-- "which requirements does this case's failure history touch") had to be
-- answered from a free-text `task_name` string instead of a stable identity.
-- `task_name` is not an identity: two different cases may share a name, and one
-- case may be renamed, so history keyed on it is not history.
--
-- The backward link already exists and is untouched: `test_cases` carries
-- `executed_run_id` / `executed_task_id` (0005_test_cases.sql), recording the
-- most recent execution of a case. This migration adds the missing forward
-- direction so every execution is reachable from its case, which is what makes
-- the existing telemetry column usable.
--
-- Safety: a nullable column plus an index. No existing row is dropped, rewritten
-- or moved, and no backfill is required for correctness - historical telemetry
-- keeps whatever it had (NULL) until the idempotent, opt-in backfill in
-- `store::backfill_task_test_case_links` recovers only the unambiguous cases.

ALTER TABLE tasks ADD COLUMN test_case_id TEXT;

-- Serves the Phase 3 per-case history reads ("every execution of this case, in
-- time order"). `run_id` leads so run-scoped task listings keep using it, and
-- `test_case_id` follows so a single-case query does not need a second lookup.
CREATE INDEX IF NOT EXISTS idx_tasks_test_case ON tasks(test_case_id, run_id);

-- The telemetry side is the table Phase 3 will actually aggregate, and it had no
-- index on test_case_id at all - only on task_name and failure_fingerprint.
CREATE INDEX IF NOT EXISTS idx_telemetry_test_case
    ON execution_telemetry(project_id, test_case_id);