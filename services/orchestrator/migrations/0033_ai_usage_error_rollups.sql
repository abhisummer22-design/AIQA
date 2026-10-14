-- Phase 3 (AI Usage Monitoring) — additive error rollup counters for ai_usage.
--
-- Tracks aggregate counts of rate limits (HTTP 429), server errors (HTTP 5xx),
-- and timeouts per 15-minute slot, project, provider, model, and task.
--
-- ADDITIVE CHANGE:
-- * `ADD COLUMN` on SQLite leaves existing rows intact with DEFAULT 0.
-- * Nothing is dropped, rewritten, or recreated.

ALTER TABLE ai_usage ADD COLUMN http_429_calls INTEGER NOT NULL DEFAULT 0;
ALTER TABLE ai_usage ADD COLUMN http_5xx_calls INTEGER NOT NULL DEFAULT 0;
ALTER TABLE ai_usage ADD COLUMN timeout_calls INTEGER NOT NULL DEFAULT 0;
