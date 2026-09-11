-- Vector Store v1 — the 2nd/3rd of the 3 "NOT IMPLEMENTED" Orchestrator
-- pipeline stages (Re-planning was the 1st, already real — see
-- `api.rs::get_neural_pipeline`'s "replanning" stage). See
-- `ai_router.rs::GeminiEmbedder`'s own doc comment for why this is
-- Gemini-only (verified live: no other configured provider — Groq, MJ
-- Gateway, Ollama — has ever exposed a real embeddings endpoint in this
-- codebase), and `ai_generation.rs::rank_similar_test_cases`'s own doc
-- comment for how this table gets read at generation time (RAG Retrieval).
--
-- One row per test case that has ever reached APPROVED (re-embedded on
-- every subsequent edit while it stays approved — see
-- `api.rs::embed_approved_test_case`'s own doc comment). A case that is
-- GENERATED/VALIDATED/REJECTED/EXECUTED-but-never-approved (impossible per
-- the real lifecycle in `0005_test_cases.sql`'s own comment — EXECUTED is
-- only reachable from APPROVED) has no row here, so retrieval can never
-- ground new generation on an unreviewed or rejected case. No FK on
-- `project_id`/`test_case_id` — same established convention every other
-- project-scoped table in this schema already follows (see
-- `0004_projects.sql`'s own comment for why); `test_cases.delete` explicitly
-- deletes the matching row here too (`store.rs::delete_test_case`), so this
-- never silently orphans.
--
-- `embedding` is stored as JSON text (a plain array of floats), not a BLOB
-- or a native vector column: SQLite has neither, and brute-force cosine
-- similarity in Rust over one project's own rows (realistically a few
-- hundred at most) is genuinely fast enough without one — see
-- `ai_generation.rs::rank_similar_test_cases`'s own doc comment for why a
-- real ANN index was deliberately not added for this v1. `model`/
-- `dimensions` are stored per row (not assumed global) so a future change
-- to `GEMINI_EMBEDDING_MODEL`/`GEMINI_EMBEDDING_DIMENSIONS` never silently
-- compares embeddings produced by two different real models/sizes against
-- each other — `rank_similar_test_cases`'s cosine-similarity math requires
-- equal-length vectors and treats a mismatch as zero similarity, never a
-- panic.
CREATE TABLE test_case_embeddings (
    test_case_id  TEXT PRIMARY KEY,
    project_id    TEXT NOT NULL,
    model         TEXT NOT NULL,
    dimensions    INTEGER NOT NULL,
    embedding     TEXT NOT NULL, -- JSON array of floats
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE INDEX idx_test_case_embeddings_project ON test_case_embeddings (project_id);
