-- Phase 5: Governed Self-Healing & Quality Feedback
-- Strongly-typed HealingProposal lifecycle persistence and strategy calibration metrics.

CREATE TABLE healing_proposals (
    id                      TEXT PRIMARY KEY,
    project_id              TEXT NOT NULL,
    run_id                  TEXT NOT NULL,
    task_id                 TEXT NOT NULL,
    failure_fingerprint     TEXT NOT NULL,
    diagnosis_category      TEXT NOT NULL,
    confidence              REAL NOT NULL,
    risk_level              TEXT NOT NULL,
    requires_approval       INTEGER NOT NULL,
    original_actions        TEXT NOT NULL,
    proposed_actions        TEXT NOT NULL,
    ast_diff                TEXT NOT NULL,
    rationale               TEXT NOT NULL,
    evidence_refs           TEXT NOT NULL DEFAULT '[]',
    state                   TEXT NOT NULL,
    idempotency_key         TEXT NOT NULL,
    healing_run_id          TEXT,
    verification_result     TEXT,
    approved_by             TEXT,
    approved_at             TEXT,
    rejection_reason        TEXT,
    created_at              TEXT NOT NULL,
    expires_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL,
    UNIQUE (project_id, idempotency_key)
);

CREATE INDEX idx_healing_proposals_project_state ON healing_proposals (project_id, state);
CREATE INDEX idx_healing_proposals_run_task ON healing_proposals (run_id, task_id);

CREATE TABLE healing_strategy_metrics (
    id                      TEXT PRIMARY KEY,
    project_id              TEXT NOT NULL,
    strategy                TEXT NOT NULL,
    failure_category        TEXT NOT NULL,
    attempts_count          INTEGER NOT NULL DEFAULT 0,
    verified_count          INTEGER NOT NULL DEFAULT 0,
    not_verified_count      INTEGER NOT NULL DEFAULT 0,
    regression_count        INTEGER NOT NULL DEFAULT 0,
    calibrated_confidence   REAL NOT NULL DEFAULT 0.5,
    updated_at              TEXT NOT NULL,
    UNIQUE (project_id, strategy, failure_category)
);

CREATE INDEX idx_healing_strategy_metrics_project ON healing_strategy_metrics (project_id);
