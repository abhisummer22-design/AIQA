-- Target-schema mirror of services/orchestrator/migrations/0008_performance_runs.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale). Phase 3 gap-closing.
--
-- Real, persisted Performance Testing (Load/Stress/Spike/Volume) run
-- history. A new, minimal, purpose-built table — the `runs`/`tasks` schema
-- (001) is shaped for fast, DAG-shaped, one-shot task dispatch, a genuine
-- lifecycle mismatch with a long-running, cancellable, live-progress
-- workload. project_id is UUID with no FK, same reasoning as
-- 004_projects.sql's own comment.
--
-- `config_json`/`metrics_json`/`verdict_reasons_json` are JSONB (a real,
-- direct upgrade from SQLite's opaque-JSON-as-TEXT workaround — same
-- reasoning as 005_test_cases.sql's own comment); the backend still stores
-- and returns them without interpreting their internal shape at the SQL
-- layer. `evidence_id` stays TEXT, not UUID/FK: it is an opaque lookup key
-- into the filesystem-backed `LocalEvidenceStore`, not a row in the
-- `evidence` table from 002 (that table is itself unwired scaffolding, per
-- 002's own header comment: "nothing in services/orchestrator populates
-- these tables yet").
--
-- `status` lifecycle: RUNNING -> COMPLETED | CANCELLED | ERROR | INTERRUPTED.
CREATE TABLE performance_runs (
    id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id        UUID NOT NULL,
    name              TEXT NOT NULL,
    test_type         TEXT NOT NULL, -- LOAD | STRESS | SPIKE | VOLUME
    status            TEXT NOT NULL DEFAULT 'RUNNING',
    verdict           TEXT,          -- COMPLETED | PASSED | FAILED | ERROR | CANCELLED, set once terminal
    config_json       JSONB NOT NULL,
    metrics_json      JSONB,         -- NULL until terminal
    verdict_reasons_json JSONB,
    error_reason      TEXT,
    evidence_id       TEXT,          -- raw per-request samples blob, once persisted
    created_by        TEXT,
    started_at        TIMESTAMPTZ NOT NULL,
    finished_at       TIMESTAMPTZ
);

CREATE INDEX idx_performance_runs_project ON performance_runs (project_id, started_at);
CREATE INDEX idx_performance_runs_project_status ON performance_runs (project_id, status);
