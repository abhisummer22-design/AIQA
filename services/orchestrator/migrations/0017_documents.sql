-- Phase 2 (document-level RAG) — the natural next step after Vector Store +
-- RAG Retrieval v1 (`0014_test_case_embeddings.sql`): that migration made a
-- project's own past APPROVED test cases retrievable as grounding at
-- generation time; this one does the same for a project's own uploaded
-- requirement documents (BRD/PRD/FRD/etc — `doc_extract.rs`'s real text
-- extraction). Before this table, `TestCaseGeneratorPage.tsx`'s own
-- `buildRequirementText()` only folded an uploaded document's text into the
-- ONE generation request it was attached to — the moment that request
-- finished, the document's content was gone from the system entirely. This
-- table is what lets a LATER, unrelated-looking generation request still
-- benefit from a document uploaded once, earlier, exactly the way an
-- earlier-approved test case already can (see `ai_generation.rs::
-- rank_similar_documents`'s own comment for how this table gets read back).
--
-- One row per real, distinct extracted document per project — deduplicated
-- by `content_hash` (a SHA-256 of the real extracted `text`, not the
-- original file bytes, so re-uploading the exact same .docx twice, or the
-- same content saved as both .docx and .md, still collapses to one row once
-- their extracted text matches) via the `UNIQUE(project_id, content_hash)`
-- constraint below. Re-uploading a document that already has a row simply
-- refreshes `file_name`/`updated_at` (`store.rs::upsert_document`'s own
-- `ON CONFLICT`), never a second, stale duplicate.
--
-- Unlike `test_case_embeddings` (a separate table, because `test_cases`
-- already existed before that feature and only a subset of cases ever earn
-- a row), `documents` is a brand-new table with no such history, so its
-- embedding columns live directly on this same row rather than in a second
-- table — one real row is simpler than a row-plus-a-matching-row-elsewhere
-- for a table that has no reader that doesn't already need the row. A fresh
-- row's `embedding`/`embedding_model`/`embedding_dimensions` are all NULL
-- until a real embedder is configured and the embed call succeeds
-- (`api.rs::persist_and_embed_document`'s own fail-open comment) — the text
-- is still stored either way, so it becomes retrievable retroactively the
-- moment an embedding provider is later configured and this document is
-- next re-uploaded.
--
-- No FK on `project_id` — same established convention every other
-- project-scoped table in this schema already follows (`0004_projects.sql`'s
-- own comment); `store.rs::delete_project` explicitly deletes this table's
-- rows for a deleted project too, so this never silently orphans.
CREATE TABLE documents (
    id                    TEXT PRIMARY KEY,
    project_id            TEXT NOT NULL,
    file_name             TEXT NOT NULL,
    content_hash          TEXT NOT NULL,
    text                  TEXT NOT NULL,
    char_count            INTEGER NOT NULL,
    truncated             INTEGER NOT NULL,
    embedding             TEXT,    -- JSON array of floats; NULL until embedded
    embedding_model       TEXT,
    embedding_dimensions  INTEGER,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    UNIQUE(project_id, content_hash)
);

CREATE INDEX idx_documents_project ON documents (project_id);
