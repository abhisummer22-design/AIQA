-- Target-schema mirror of services/orchestrator/migrations/0013_custom_ai_providers.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale). Phase 3 gap-closing.
--
-- Real, persisted custom AI providers added by a user through the Settings
-- UI — same durable lifetime as `ai_settings`/`custom_skills`. `api_key` is
-- stored in plaintext, same real exposure level a real deployment's `.env`
-- already has for its env-var-only providers (never transmitted anywhere
-- but the real provider's own API, never logged) — this is not the
-- one-way-hashed `api_keys.key_hash` pattern, because this key must be
-- read back and actually used to authenticate real outbound requests, not
-- merely verified.
CREATE TABLE custom_ai_providers (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    base_url    TEXT NOT NULL,
    api_key     TEXT NOT NULL,
    created_by  TEXT NOT NULL,     -- api_keys.name of the key that created it
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
