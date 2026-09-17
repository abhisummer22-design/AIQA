-- Target-schema mirror of services/orchestrator/migrations/0009_custom_skills.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale). Phase 3 gap-closing. `metadata` is intentionally NOT
-- included here — it is added later, by 010_custom_skill_metadata.sql, in
-- the same order the real schema actually acquired it.
--
-- Real, persisted custom skills for AI test-case generation. `description`
-- is the exact text embedded verbatim into the generation prompt for any
-- skill name not recognized as built-in.
CREATE TABLE custom_skills (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    description TEXT NOT NULL,
    source_file TEXT,              -- original uploaded file name, if any (evidence only, not re-read)
    created_by  TEXT NOT NULL,     -- api_keys.name of the key that created it
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
