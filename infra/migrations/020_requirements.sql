-- Target-schema mirror of services/orchestrator/migrations/0020_requirements.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale).
--
-- A document's full extracted text (keyed by a SHA-256 of that text) and the
-- individual requirements read out of it, each with the id people see, the
-- section it sits in and a kind. `skipped_sections` is JSONB here (a direct
-- upgrade from SQLite's TEXT-as-JSON workaround).
CREATE TABLE requirement_documents (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id        UUID NOT NULL,
    file_name         TEXT NOT NULL,
    doc_type          TEXT,
    content_hash      TEXT NOT NULL,
    char_count        BIGINT NOT NULL,
    text              TEXT NOT NULL,
    parser_version    INTEGER NOT NULL,
    sections_count    INTEGER NOT NULL DEFAULT 0,
    skipped_sections  JSONB NOT NULL DEFAULT '[]',
    furniture_lines   INTEGER NOT NULL DEFAULT 0,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, content_hash)
);

CREATE INDEX idx_requirement_documents_project ON requirement_documents (project_id, created_at);

CREATE TABLE requirements (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id   UUID NOT NULL,
    document_id  UUID NOT NULL REFERENCES requirement_documents(id) ON DELETE CASCADE,
    req_key      TEXT NOT NULL,
    ordinal      INTEGER NOT NULL,
    section      TEXT,
    kind         TEXT NOT NULL,
    text         TEXT NOT NULL,
    UNIQUE (document_id, ordinal)
);

CREATE INDEX idx_requirements_project_document ON requirements (project_id, document_id, ordinal);
CREATE INDEX idx_requirements_key ON requirements (project_id, req_key);
