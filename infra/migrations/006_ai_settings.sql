-- Target-schema mirror of services/orchestrator/migrations/0006_ai_settings.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale). Phase 3 gap-closing. `task_overrides` is intentionally
-- NOT included here — it is added later, by
-- 012_ai_settings_task_overrides.sql, in the same order the real schema
-- actually acquired it.
--
-- Central AI Provider & Model Intelligence Layer — persisted routing
-- settings. Platform-wide, not project-scoped: a real singleton row, `id`
-- fixed to 'global', enforced by a CHECK constraint rather than trusting
-- application code alone to never insert a second row.
CREATE TABLE ai_settings (
    id                  TEXT PRIMARY KEY CHECK (id = 'global'),
    mode                TEXT NOT NULL DEFAULT 'AUTO',
    preferred_provider  TEXT,
    preferred_model     TEXT,
    fallback_enabled    BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at          TIMESTAMPTZ NOT NULL
);
