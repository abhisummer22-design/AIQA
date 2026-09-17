-- Target-schema mirror of services/orchestrator/migrations/0015_governed_healing.sql
-- (the real, running SQLite migration). Phase 3 gap-closing.
--
-- Strongly-typed HealingProposal lifecycle persistence and strategy
-- calibration metrics. `project_id`/`run_id`/`task_id` follow this schema's
-- existing conventions: `project_id` is UUID with no FK
-- (004_projects.sql's own comment); `task_id` stays TEXT, not UUID — a
-- plan-scoped id, not necessarily a UUID (see `tasks.id` in 001).
-- `original_actions`/`proposed_actions`/`ast_diff`/`evidence_refs` are
-- JSONB (a real, direct upgrade from SQLite's TEXT-as-JSON workaround —
-- same reasoning as 005_test_cases.sql's own comment).
CREATE TABLE healing_proposals (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id              UUID NOT NULL,
    run_id                  UUID NOT NULL,
    task_id                 TEXT NOT NULL,
    failure_fingerprint     TEXT NOT NULL,
    diagnosis_category      TEXT NOT NULL,
    confidence              NUMERIC NOT NULL,
    risk_level              TEXT NOT NULL,
    requires_approval       BOOLEAN NOT NULL,
    original_actions        JSONB NOT NULL,
    proposed_actions        JSONB NOT NULL,
    ast_diff                TEXT NOT NULL,
    rationale               TEXT NOT NULL,
    evidence_refs           JSONB NOT NULL DEFAULT '[]',
    state                   TEXT NOT NULL,
    idempotency_key         TEXT NOT NULL,
    healing_run_id          UUID,
    verification_result     TEXT,
    approved_by             TEXT,
    approved_at             TIMESTAMPTZ,
    rejection_reason        TEXT,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at              TIMESTAMPTZ NOT NULL,
    updated_at              TIMESTAMPTZ NOT NULL,
    UNIQUE (project_id, idempotency_key)
);

CREATE INDEX idx_healing_proposals_project_state ON healing_proposals (project_id, state);
CREATE INDEX idx_healing_proposals_run_task ON healing_proposals (run_id, task_id);

CREATE TABLE healing_strategy_metrics (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id              UUID NOT NULL,
    strategy                TEXT NOT NULL,
    failure_category        TEXT NOT NULL,
    attempts_count          INTEGER NOT NULL DEFAULT 0,
    verified_count          INTEGER NOT NULL DEFAULT 0,
    not_verified_count      INTEGER NOT NULL DEFAULT 0,
    regression_count        INTEGER NOT NULL DEFAULT 0,
    calibrated_confidence   NUMERIC NOT NULL DEFAULT 0.5,
    updated_at              TIMESTAMPTZ NOT NULL,
    UNIQUE (project_id, strategy, failure_category)
);

CREATE INDEX idx_healing_strategy_metrics_project ON healing_strategy_metrics (project_id);
