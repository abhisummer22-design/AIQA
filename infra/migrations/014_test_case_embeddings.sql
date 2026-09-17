-- Target-schema mirror of services/orchestrator/migrations/0014_test_case_embeddings.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale). Phase 3 gap-closing.
--
-- Vector Store + RAG Retrieval v1 — one row per test case that has ever
-- reached APPROVED (re-embedded on every subsequent edit while it stays
-- approved). No FK on `project_id`/`test_case_id`, matching the real
-- SQLite schema's own deliberate choice (see that migration's own comment).
--
-- `embedding` uses a real pgvector `vector(768)` column, not JSONB/TEXT —
-- SQLite has neither a vector type nor the extension, hence its own
-- JSON-array-as-TEXT workaround with application-side brute-force cosine
-- similarity; Postgres genuinely has both, via the `vector` extension
-- 002_quality_graph_and_intelligence.sql already enables. `768`, not
-- 002's own `quality_nodes.embedding vector(1536)`: that column was a
-- generic placeholder dimension predating any real embedding provider
-- decision, whereas this one matches the real, currently-working
-- provider's own default — Gemini's `gemini-embedding-001`, Matryoshka-
-- truncated to 768 (`ai_router.rs::GeminiEmbedder`, `unwrap_or(768)`) —
-- not a guess. Real caveat a `vector(N)` column has that SQLite's schema-
-- less blob does not: `GEMINI_EMBEDDING_DIMENSIONS` is a real, configurable
-- env var, so a deployment that overrides it away from the default would
-- need this column's declared width updated to match, or every insert of a
-- differently-sized vector fails outright (`dimensions` here still records
-- the real, actual size actually used, same as the SQLite column already
-- does, independent of what this column's own fixed width happens to be).
CREATE TABLE test_case_embeddings (
    test_case_id  UUID PRIMARY KEY,
    project_id    UUID NOT NULL,
    model         TEXT NOT NULL,
    dimensions    INTEGER NOT NULL,
    embedding     vector(768) NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL
);

CREATE INDEX idx_test_case_embeddings_project ON test_case_embeddings (project_id);
