-- 0016_adaptive_intelligence.sql
-- Phase 6: Adaptive QA Intelligence & Production Observability
-- Stores normalized execution telemetry for historical flakiness, failure clustering,
-- strategy reliability, and planning feedback.

CREATE TABLE IF NOT EXISTS execution_telemetry (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL,
    run_id TEXT NOT NULL,
    task_id TEXT NOT NULL,
    task_name TEXT NOT NULL,
    test_case_id TEXT,
    execution_id TEXT,
    executor TEXT NOT NULL,
    environment TEXT NOT NULL DEFAULT 'test',
    start_time TEXT NOT NULL,
    end_time TEXT,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 0,
    healing_attempt_count INTEGER NOT NULL DEFAULT 0,
    failure_category TEXT,
    failure_fingerprint TEXT,
    error_message TEXT,
    evidence_refs TEXT NOT NULL DEFAULT '[]',
    assertion_result TEXT,
    http_status INTEGER,
    selector TEXT,
    verification_outcome TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_telemetry_project_task
    ON execution_telemetry(project_id, task_name);

CREATE INDEX IF NOT EXISTS idx_telemetry_fingerprint
    ON execution_telemetry(project_id, failure_fingerprint);

CREATE INDEX IF NOT EXISTS idx_telemetry_created_at
    ON execution_telemetry(project_id, created_at);

CREATE INDEX IF NOT EXISTS idx_telemetry_run_id
    ON execution_telemetry(run_id);

CREATE INDEX IF NOT EXISTS idx_telemetry_env
    ON execution_telemetry(project_id, environment);
