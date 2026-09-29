-- Test Execution display preference: whether a real browser window is shown on screen while a
-- Playwright run drives it, instead of the headless run every earlier release always used. This
-- is genuinely platform-wide (the executor that reads it has no project context), so — same
-- reasoning migration 0006 gives for `ai_settings` — this is a real singleton row, `id` fixed to
-- 'global' by a CHECK constraint rather than trusting application code alone.
CREATE TABLE execution_display_settings (
    id                    TEXT PRIMARY KEY CHECK (id = 'global'),
    show_browser_window   INTEGER NOT NULL DEFAULT 0,
    updated_at            TEXT NOT NULL
);
