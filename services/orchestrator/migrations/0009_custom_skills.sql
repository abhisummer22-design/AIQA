-- Real, persisted custom skills for AI test-case generation. Previously
-- session-only (frontend `useSkillSelection.ts`, gone on reload) — this
-- table gives them the same durable lifetime as everything else a user
-- configures (projects, AI settings). `description` is the exact text
-- `ai_generation.rs::resolve_skill_instructions` embeds verbatim into the
-- generation prompt for any skill name it does not recognize as built-in,
-- so a real, filled-out description here genuinely changes generated output
-- — not just a label change.
CREATE TABLE custom_skills (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT NOT NULL,
    source_file TEXT,              -- original uploaded file name, if any (evidence only, not re-read)
    created_by  TEXT NOT NULL,     -- api_keys.name of the key that created it
    created_at  TEXT NOT NULL
);
