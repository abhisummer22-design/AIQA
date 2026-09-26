-- API tests made from an OpenAPI document, and the run each was executed as (`openapi_runs.rs`).
--
-- A run of these tests is an ordinary execution (`run_id`: one task for each case that is run, so it has its evidence, its audit
-- trail and its place in the Execution Hub like any other). This table only remembers what that run was made from, so that a report
-- can say, for each operation of the document, what was expected and what came back: the cases as they were derived (`cases`, each
-- with the task that ran it), the operations the document has (`operations`, and which of them were asked for) and the options.
--
-- What is stored is never a credential: a header that carries one is written `env:NAME` (the value is read from the orchestrator's
-- environment when a request is made), and the derived requests hold values made up from the schemas, not real data. The document
-- itself is not stored: the same derivation from it and the options gives the same cases.
CREATE TABLE openapi_runs (
    id            TEXT PRIMARY KEY,
    project_id    TEXT NOT NULL,
    run_id        TEXT NOT NULL,
    title         TEXT NOT NULL,
    version       TEXT NOT NULL,
    base_url      TEXT NOT NULL,
    options       TEXT NOT NULL,
    operations    TEXT NOT NULL,
    cases         TEXT NOT NULL,
    notes         TEXT NOT NULL DEFAULT '[]',
    created_by    TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

CREATE INDEX idx_openapi_runs_project ON openapi_runs (project_id, created_at);
