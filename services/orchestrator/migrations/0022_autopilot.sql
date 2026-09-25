-- Autopilot: a run that takes a project from its uploaded requirements to a report of executed tests (`autopilot.rs`).
--
-- The database is the run's only memory. A run is a row here, each of its steps is a row, and every checkpoint a step needs
-- to carry on after the service was restarted (the crawl it started, the requirements it has generated cases for) is
-- written into the step's `detail` as it happens, so a run that was going when the process stopped is picked up again at
-- startup and neither repeats the work already done nor starts a second crawl.
--
-- `config` is what the person asked for and `estimate` is what it was going to cost when they were shown it: a run starts
-- only after the estimate is accepted (`approved_at`), and a run that would cost money only when that was accepted too
-- (`approved_paid`). Login steps in `config` refer to secrets by name (`env:NAME`), never by value.
--
-- Every call the run makes to this service's own API is made with the identity of the key that created it
-- (`created_by_key`), looked up again before each call: a key that has been revoked stops its runs.
--
-- No FK on `project_id`, same convention as every project-scoped table here (`0004_projects.sql`);
-- `store.rs::delete_project` deletes these rows.
CREATE TABLE autopilot_runs (
    id                TEXT PRIMARY KEY,
    project_id        TEXT NOT NULL,
    name              TEXT NOT NULL,
    state             TEXT NOT NULL,             -- AWAITING_APPROVAL | RUNNING | WAITING | COMPLETED | FAILED | CANCELLED
    config            TEXT NOT NULL,             -- JSON, `autopilot::AutopilotConfig`
    estimate          TEXT NOT NULL,             -- JSON, `autopilot_estimate::Estimate`, as it was shown to the person
    approved_at       TEXT,                      -- when the estimate was accepted
    approved_paid     INTEGER NOT NULL DEFAULT 0,
    created_by_key    TEXT NOT NULL,             -- the API key whose identity every call of the run is made with
    created_by_name   TEXT NOT NULL,
    current_step      TEXT,
    error             TEXT,                      -- why a FAILED run failed
    cancel_requested  INTEGER NOT NULL DEFAULT 0,
    created_at        TEXT NOT NULL,
    started_at        TEXT,
    finished_at       TEXT,
    updated_at        TEXT NOT NULL
);

CREATE INDEX idx_autopilot_runs_project ON autopilot_runs (project_id, created_at);
CREATE INDEX idx_autopilot_runs_state ON autopilot_runs (state);

CREATE TABLE autopilot_steps (
    run_id       TEXT NOT NULL REFERENCES autopilot_runs(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,                  -- REQUIREMENTS | CRAWL | GAP_CHECK | ...
    position     INTEGER NOT NULL,               -- order within the run, from 0
    state        TEXT NOT NULL,                  -- PENDING | RUNNING | WAITING | DONE | SKIPPED | FAILED
    attempts     INTEGER NOT NULL DEFAULT 0,     -- how many times the step was started (a restart starts it again)
    detail       TEXT NOT NULL DEFAULT '{}',     -- JSON: what the step found, and the checkpoints it resumes from
    error        TEXT,
    started_at   TEXT,
    finished_at  TEXT,
    PRIMARY KEY (run_id, name)
);

-- What happened, in order: the run's timeline as the person reads it.
CREATE TABLE autopilot_events (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id   TEXT NOT NULL REFERENCES autopilot_runs(id) ON DELETE CASCADE,
    at       TEXT NOT NULL,
    kind     TEXT NOT NULL,                      -- run.created | step.started | step.done | ...
    step     TEXT,
    message  TEXT NOT NULL,
    data     TEXT                                -- JSON, optional
);

CREATE INDEX idx_autopilot_events_run ON autopilot_events (run_id, id);
