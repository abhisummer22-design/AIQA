-- SQLite mirror of infra/migrations/001_core_execution.sql (FRD §2, SRD §2,
-- §4-5, §19), adapted for the embedded engine actually running in this slice.
--
-- Why SQLite and not the Postgres target schema directly: this dev
-- environment has no container runtime (no `docker`/`podman` on PATH — see
-- README "Persistence" section), so the `infra/docker-compose.yml` Postgres
-- instance cannot be started here. SQLite gives real, durable, restart-
-- surviving persistence without that dependency. Column shapes intentionally
-- track the Postgres migration (same names, same semantics) so a future swap
-- to `Postgres` in `store.rs` is a driver change, not a redesign.
--
-- Deviations from the Postgres schema, and why:
--   * UUID/TIMESTAMPTZ/JSONB -> TEXT (SQLite has no native UUID/JSONB/tz
--     type; values are still UUID strings / RFC3339 strings / JSON text).
--   * ENUM -> TEXT (SQLite has no CREATE TYPE); validity is enforced in Rust
--     (domain.rs), same as it already was for the in-memory version.
--   * gen_random_uuid() default removed — the Rust layer always generates
--     the id before insert (uuid::Uuid::new_v4()), so no DB-side default is
--     needed.

CREATE TABLE runs (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    project_id      TEXT,
    state           TEXT NOT NULL DEFAULT 'CREATED',
    concurrency     INTEGER NOT NULL DEFAULT 4,
    plan            TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    started_at      TEXT,
    finished_at     TEXT
);

CREATE TABLE tasks (
    id              TEXT NOT NULL,
    run_id          TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    name            TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'QUEUED',
    reason          TEXT,
    executor_kind   TEXT,
    started_at      TEXT,
    finished_at     TEXT,
    PRIMARY KEY (run_id, id)
);

CREATE TABLE task_dependencies (
    run_id          TEXT NOT NULL,
    task_id         TEXT NOT NULL,
    prerequisite_id TEXT NOT NULL,
    PRIMARY KEY (run_id, task_id, prerequisite_id)
);

CREATE TABLE run_events (
    event_id        TEXT PRIMARY KEY,
    run_id          TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    task_id         TEXT,
    sequence        INTEGER NOT NULL,
    event_type      TEXT NOT NULL,
    status          TEXT,
    summary         TEXT,
    schema_version  INTEGER NOT NULL DEFAULT 1,
    occurred_at     TEXT NOT NULL,
    UNIQUE (run_id, sequence)
);
CREATE INDEX idx_run_events_run_id ON run_events (run_id, sequence);
