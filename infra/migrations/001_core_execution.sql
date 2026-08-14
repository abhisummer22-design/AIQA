-- Core execution schema: Run/Task/Event model (FRD §2, SRD §2, §4-5, §19).
-- NOT YET WIRED into services/orchestrator, which persists in-memory for this
-- slice — this is the target schema for when that persistence work starts.

CREATE TYPE run_state AS ENUM (
    'CREATED', 'PLANNING', 'SCHEDULED', 'RUNNING', 'PARTIAL',
    'FINALIZING', 'COMPLETED', 'CANCELLED', 'FAILED'
);

CREATE TYPE task_status AS ENUM (
    'QUEUED', 'RUNNING', 'PASSED', 'FAILED', 'BLOCKED', 'SKIPPED',
    'CANCELLED', 'TIMEOUT', 'INFRA_ERROR', 'NEEDS_REVIEW'
);

CREATE TABLE runs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT NOT NULL,
    project_id      UUID,                       -- FK to a future projects table
    state           run_state NOT NULL DEFAULT 'CREATED',
    concurrency     INTEGER NOT NULL DEFAULT 4,
    plan            JSONB NOT NULL,              -- the submitted RunPlan, for audit/replay
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    started_at      TIMESTAMPTZ,
    finished_at     TIMESTAMPTZ
);

CREATE TABLE tasks (
    id              TEXT NOT NULL,               -- plan-scoped id, unique per run
    run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    status          task_status NOT NULL DEFAULT 'QUEUED',
    reason          TEXT,                        -- e.g. why BLOCKED/FAILED/INFRA_ERROR
    executor_kind   TEXT,                        -- 'playwright' | 'maestro' | 'newman' | ... (SRD §16-17)
    started_at      TIMESTAMPTZ,
    finished_at     TIMESTAMPTZ,
    PRIMARY KEY (run_id, id)
);

CREATE TABLE task_dependencies (
    run_id          UUID NOT NULL,
    task_id         TEXT NOT NULL,
    prerequisite_id TEXT NOT NULL,
    PRIMARY KEY (run_id, task_id, prerequisite_id),
    FOREIGN KEY (run_id, task_id) REFERENCES tasks(run_id, id) ON DELETE CASCADE,
    FOREIGN KEY (run_id, prerequisite_id) REFERENCES tasks(run_id, id) ON DELETE CASCADE
);

-- SRD §5 Event Ordering & Idempotency: event_id, run_id, task_id, sequence,
-- timestamp, schema_version on every event; (run_id, sequence) is the
-- idempotency key consumers dedupe on.
CREATE TABLE run_events (
    event_id        UUID PRIMARY KEY,
    run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    task_id         TEXT,
    sequence        BIGINT NOT NULL,
    event_type      TEXT NOT NULL,
    status          task_status,
    summary         TEXT,
    schema_version  INTEGER NOT NULL DEFAULT 1,
    occurred_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (run_id, sequence)
);
CREATE INDEX idx_run_events_run_id ON run_events (run_id, sequence);

-- SRD §7 Parallelism & Resource Locks: exclusive resources such as a device,
-- account, database fixture, or environment-wide mutation.
CREATE TABLE resource_locks (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    resource_key    TEXT NOT NULL,               -- e.g. 'device:pixel-7-emu-1'
    run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    task_id         TEXT,
    acquired_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    released_at     TIMESTAMPTZ
);
CREATE UNIQUE INDEX idx_resource_locks_active
    ON resource_locks (resource_key) WHERE released_at IS NULL;

-- SRD §9 / PRD §18 Environment Intelligence: fingerprint captured per run/task.
CREATE TABLE environment_fingerprints (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id          UUID NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    task_id         TEXT,
    app_version     TEXT,
    browser_device  TEXT,
    os              TEXT,
    api_version     TEXT,
    feature_flags   JSONB,
    dependencies    JSONB,
    config_refs     JSONB,
    infra_metadata  JSONB,
    captured_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
