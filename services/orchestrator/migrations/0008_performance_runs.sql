-- Phase 6 — real, persisted Performance Testing (Load/Stress/Spike/Volume)
-- run history.
--
-- Fresh baseline audit before this migration (see
-- AIQA_PHASE_6_PERFORMANCE_TESTING_IMPLEMENTATION_REPORT.md §2/§3): no
-- existing table stores "many real per-request samples aggregated into one
-- run" — `evidence` is one blob per task, `failure_clusters` aggregates the
-- opposite direction. Rather than force a long-running, cancellable,
-- live-progress workload through the `runs`/`tasks` schema (built for
-- fast, DAG-shaped, one-shot task dispatch — a genuine lifecycle mismatch),
-- this is a new, minimal, purpose-built table, project-scoped the same
-- free-text way every other project-scoped table has been since
-- 0004_projects.sql (no FK, same reasoning as that migration's own note).
--
-- `config_json`/`metrics_json` are opaque JSON text, same convention this
-- schema already uses for `test_cases.traceability` etc — the backend
-- stores and returns them without interpreting their internal shape at the
-- SQL layer (interpretation lives in `performance_testing.rs`'s typed
-- Rust structs).
--
-- Raw per-request samples are NOT duplicated into this row — they are
-- persisted once, as a JSON blob, via the *existing* `LocalEvidenceStore`
-- (`evidence_id` below points at that blob), reusing the same
-- evidence-store convention `api_testing.rs`/`CompiledApiExecutor` already
-- established rather than inventing a second large-blob storage path.
--
-- `status` lifecycle: RUNNING -> COMPLETED | CANCELLED | ERROR | INTERRUPTED.
-- INTERRUPTED is set only by the startup recovery sweep
-- (`performance_testing::recover_incomplete_performance_runs`, mirroring
-- `engine::recover_incomplete_runs`) for a row still RUNNING when the
-- process restarts — an in-flight workload's live, in-memory state
-- (`AppState::performance_tests`) does not survive a restart, so a prior
-- process's abandoned run is honestly marked, never silently left RUNNING
-- forever nor guessed at as COMPLETED.
CREATE TABLE performance_runs (
    id                TEXT PRIMARY KEY,
    project_id        TEXT NOT NULL,
    name              TEXT NOT NULL,
    test_type         TEXT NOT NULL, -- LOAD | STRESS | SPIKE | VOLUME
    status            TEXT NOT NULL DEFAULT 'RUNNING',
    verdict           TEXT,          -- COMPLETED | PASSED | FAILED | ERROR | CANCELLED, set once terminal
    config_json       TEXT NOT NULL,
    metrics_json      TEXT,          -- NULL until terminal
    verdict_reasons_json TEXT,
    error_reason      TEXT,
    evidence_id       TEXT,          -- raw per-request samples blob, once persisted
    created_by        TEXT,
    started_at        TEXT NOT NULL,
    finished_at       TEXT
);

CREATE INDEX idx_performance_runs_project ON performance_runs (project_id, started_at);
CREATE INDEX idx_performance_runs_project_status ON performance_runs (project_id, status);
