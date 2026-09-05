-- Real, persisted custom AI providers added by a user through the Settings
-- UI (brief §21's own "Settings UI -> Settings API -> Persistent
-- Configuration -> AI Router" pattern, applied to providers instead of
-- routing settings) — same durable lifetime as `ai_settings`/
-- `custom_skills`, so a real provider a user adds survives a restart
-- without needing to be re-entered, unlike the env-var-only providers
-- (Ollama/MJ Gateway/Groq/Gemini) which still require an actual process
-- restart to pick up a changed key. `api_key` is stored in plaintext, same
-- real exposure level `.env` already has for those same env-var providers
-- on this machine (never transmitted anywhere but the real provider's own
-- API, never logged — see `sanitize_log_message`) — this is not the
-- one-way-hashed `api_keys.key_hash` pattern, because this key must be
-- read back and actually used to authenticate real outbound requests, not
-- merely verified.
CREATE TABLE custom_ai_providers (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    base_url    TEXT NOT NULL,
    api_key     TEXT NOT NULL,
    created_by  TEXT NOT NULL,     -- api_keys.name of the key that created it
    created_at  TEXT NOT NULL
);
