-- Central AI Provider & Model Intelligence Layer — persisted routing
-- settings (brief §21: "Preferred architecture: Settings UI -> Settings
-- API -> Persistent Configuration -> AI Router"). This is genuinely
-- platform-wide, not project-scoped data (the router itself is described
-- as "the central AI infrastructure for the entire AIQA platform"), so
-- this is a real singleton row — id is always the fixed string 'global',
-- enforced by a CHECK constraint rather than trusting application code
-- alone to never insert a second row.
CREATE TABLE ai_settings (
    id                  TEXT PRIMARY KEY CHECK (id = 'global'),
    mode                TEXT NOT NULL DEFAULT 'AUTO',
    preferred_provider  TEXT,
    preferred_model     TEXT,
    fallback_enabled    INTEGER NOT NULL DEFAULT 1,
    updated_at          TEXT NOT NULL
);
