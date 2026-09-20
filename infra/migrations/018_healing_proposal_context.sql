-- Target-schema mirror of services/orchestrator/migrations/0018_healing_proposal_context.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale).
--
-- What a healing proposal needs in order to be executed later (executor,
-- environment, task name, target URL, timeout, requirement reference, run
-- parameters), which the table had no column for. JSONB here (a real, direct
-- upgrade from SQLite's TEXT-as-JSON workaround). NULL means the row predates
-- this column and is rebuilt with fixed defaults.
ALTER TABLE healing_proposals ADD COLUMN context JSONB;
