-- Target-schema mirror of services/orchestrator/migrations/0017_documents.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale). Phase 3 gap-closing.
--
-- Phase 2 (document-level RAG) — one row per real, distinct extracted
-- document per project, deduplicated by a SHA-256 `content_hash` of its own
-- extracted text. No FK on `project_id`, matching this schema's established
-- convention (004_projects.sql's own comment).
--
-- `embedding` uses a real pgvector `vector(768)` column, matching
-- 014_test_case_embeddings.sql's own reasoning (and its own disclosed
-- caveat about `GEMINI_EMBEDDING_DIMENSIONS` being real and configurable,
-- unlike SQLite's schema-less blob) exactly: sized to the real,
-- currently-working Gemini embedder's own default (768), not a placeholder.
-- `embedding_model`/`embedding_dimensions` stay nullable alongside it —
-- NULL until a real embedder is configured and a real embed call for this
-- document has succeeded (see the real SQLite migration's own comment on
-- why the text is still stored either way).
CREATE TABLE documents (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id            UUID NOT NULL,
    file_name             TEXT NOT NULL,
    content_hash          TEXT NOT NULL,
    text                  TEXT NOT NULL,
    char_count            INTEGER NOT NULL,
    truncated             BOOLEAN NOT NULL,
    embedding             vector(768),
    embedding_model       TEXT,
    embedding_dimensions  INTEGER,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at            TIMESTAMPTZ NOT NULL,
    UNIQUE (project_id, content_hash)
);

CREATE INDEX idx_documents_project ON documents (project_id);
