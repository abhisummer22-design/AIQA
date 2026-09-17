-- Target-schema mirror of services/orchestrator/migrations/0016_adaptive_intelligence.sql
-- (the real, running SQLite migration). Phase 3 gap-closing.
--
-- Normalized execution telemetry for historical flakiness, failure
-- clustering, strategy reliability, and planning feedback. `test_case_id`
-- is UUID (a real test_cases.id when set, no FK — matching this schema's
-- established "no FK on a project-scoped cross-reference" convention);
-- `execution_id` stays TEXT — not a value this codebase guarantees is
-- UUID-shaped. `evidence_refs` is JSONB (real upgrade from SQLite's
-- TEXT-as-JSON workaround, same reasoning as 005_test_cases.sql's own
-- comment).
CREATE TABLE execution_telemetry (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id UUID NOT NULL,
    run_id UUID NOT NULL,
    task_id TEXT NOT NULL, -- plan-scoped id, not necessarily a UUID (see tasks.id in 001)
    task_name TEXT NOT NULL,
    test_case_id UUID,
    execution_id TEXT,
    executor TEXT NOT NULL,
    environment TEXT NOT NULL DEFAULT 'test',
    start_time TIMESTAMPTZ NOT NULL,
    end_time TIMESTAMPTZ,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    status TEXT NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 0,
    healing_attempt_count INTEGER NOT NULL DEFAULT 0,
    failure_category TEXT,
    failure_fingerprint TEXT,
    error_message TEXT,
    evidence_refs JSONB NOT NULL DEFAULT '[]',
    assertion_result TEXT,
    http_status INTEGER,
    selector TEXT,
    verification_outcome TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_telemetry_project_task
    ON execution_telemetry(project_id, task_name);

CREATE INDEX idx_telemetry_fingerprint
    ON execution_telemetry(project_id, failure_fingerprint);

CREATE INDEX idx_telemetry_created_at
    ON execution_telemetry(project_id, created_at);

CREATE INDEX idx_telemetry_run_id
    ON execution_telemetry(run_id);

CREATE INDEX idx_telemetry_env
    ON execution_telemetry(project_id, environment);
