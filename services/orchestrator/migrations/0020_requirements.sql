-- Requirement documents and the individual requirements read out of them.
--
-- `documents` (0017) keeps the first 20,000 characters of an uploaded file for
-- similarity retrieval and can hold one row per distinct *prefix*. This pair of
-- tables is for the whole document: `requirement_documents` keeps a document's
-- full extracted text once, keyed by a SHA-256 of that full text (so two
-- versions of a long FRD that differ only after the first 20,000 characters
-- are two rows, and re-uploading identical content refreshes one row), and
-- `requirements` keeps what `requirements.rs` read out of it — one row per
-- requirement, with the id people see (the document's own `FR-012`, or a
-- generated `FRD-3.2-04`), the section it sits in and a kind.
--
-- Parsing is deterministic and needs no AI provider. `parser_version` records
-- which rules produced the rows, so a document whose rows came from older
-- rules is parsed again the next time the same content is uploaded, and
-- unchanged content is never parsed twice.
--
-- A test case ties itself to a requirement through its existing
-- `requirement_reference` column (set to the requirement's `req_key`), which is
-- what the quality graph's `validates` edge and `GET .../requirement-impact`
-- already read; nothing new is needed on `test_cases`.
--
-- No FK on `project_id` — same convention as every project-scoped table here
-- (`0004_projects.sql`); `store.rs::delete_project` deletes these rows.
CREATE TABLE requirement_documents (
    id                TEXT PRIMARY KEY,
    project_id        TEXT NOT NULL,
    file_name         TEXT NOT NULL,
    doc_type          TEXT,                       -- BRD / PRD / FRD / SRD / ADDITIONAL, as uploaded
    content_hash      TEXT NOT NULL,              -- SHA-256 of the full extracted text
    char_count        INTEGER NOT NULL,
    text              TEXT NOT NULL,              -- the full extracted text, never cut
    parser_version    INTEGER NOT NULL,
    sections_count    INTEGER NOT NULL DEFAULT 0,
    skipped_sections  TEXT NOT NULL DEFAULT '[]', -- JSON array of section titles left out on purpose
    furniture_lines   INTEGER NOT NULL DEFAULT 0, -- page numbers / running headers dropped
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    UNIQUE (project_id, content_hash)
);

CREATE INDEX idx_requirement_documents_project ON requirement_documents (project_id, created_at);

CREATE TABLE requirements (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL,
    document_id  TEXT NOT NULL REFERENCES requirement_documents(id) ON DELETE CASCADE,
    req_key      TEXT NOT NULL,      -- 'FR-012' (the document's own id) or 'FRD-3.2-04' (generated)
    ordinal      INTEGER NOT NULL,   -- position in the document, from 0
    section      TEXT,               -- the heading it sits under, e.g. '3.2 Login'
    kind         TEXT NOT NULL,      -- functional | security | performance | usability | interface | data | context | reference
    text         TEXT NOT NULL,
    UNIQUE (document_id, ordinal)
);

CREATE INDEX idx_requirements_project_document ON requirements (project_id, document_id, ordinal);
CREATE INDEX idx_requirements_key ON requirements (project_id, req_key);
