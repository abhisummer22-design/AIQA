-- Scheduled regression (`regression.rs`, `regression_runner.rs`): the cases a project has approved and run once are run again on a
-- schedule, and each run is compared with the last.
--
-- A schedule says which cases (a suite, or every case of the project), how often, and whose key it runs as. `anchor_at` is the first
-- time it is due and the grid the later ones fall on (`anchor_at` + a whole number of `every_minutes`), so a schedule keeps its hour of
-- the day however long a run takes or the service was down; `next_run_at` is the next slot on that grid.
--
-- A regression run is an ordinary execution (`run_id`, with its evidence, audit trail and place in the Execution Hub) made of the same
-- tasks a case was last run with, so nothing is compiled again and no AI is asked. This table only remembers which cases it repeated
-- (each with the run its task was copied from) and which it could not and why, so that the report can say, for each case, what came of
-- it now and what came of it last time. The results themselves are the execution's tasks: nothing is stored twice, and no credential
-- is stored (a task holds `env:NAME` references, read when it runs).
CREATE TABLE regression_schedules (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL,
    name            TEXT NOT NULL,
    suite_id        TEXT,
    every_minutes   INTEGER NOT NULL,
    anchor_at       TEXT NOT NULL,
    next_run_at     TEXT NOT NULL,
    concurrency     INTEGER NOT NULL DEFAULT 2,
    enabled         INTEGER NOT NULL DEFAULT 1,
    last_started_at TEXT,
    last_note       TEXT,
    created_by_key  TEXT NOT NULL,
    created_by      TEXT NOT NULL,
    created_at      TEXT NOT NULL
);

CREATE INDEX idx_regression_schedules_project ON regression_schedules (project_id, created_at);
CREATE INDEX idx_regression_schedules_due ON regression_schedules (enabled, next_run_at);

CREATE TABLE regression_runs (
    id            TEXT PRIMARY KEY,
    schedule_id   TEXT NOT NULL,
    project_id    TEXT NOT NULL,
    run_id        TEXT NOT NULL,
    trigger_kind  TEXT NOT NULL,
    cases         TEXT NOT NULL,
    skipped       TEXT NOT NULL DEFAULT '[]',
    created_by    TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE INDEX idx_regression_runs_schedule ON regression_runs (schedule_id, created_at);
CREATE INDEX idx_regression_runs_project ON regression_runs (project_id);
