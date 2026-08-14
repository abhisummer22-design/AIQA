-- SQLite mirror of infra/migrations/002_quality_graph_and_intelligence.sql
-- (PRD §8-19, SRD §10-14, §19), populated by the heuristic intelligence
-- layer in `intelligence.rs`.
--
-- Explicitly NOT populated by an LLM in this slice: Ollama/LLM integration
-- is out of scope for this session by direction (see README "Intelligence
-- layer"), and the PRD critique already flagged Ollama-for-reasoning as
-- unproven with no fallback. `root_cause_hypothesis` here is filled by a
-- deterministic rule-based classifier instead — real signal, explicitly
-- lower-confidence than an LLM-derived one, never silently conflated with
-- one (see `confidence` column and `intelligence.rs` doc comment).
--
-- `embedding vector(1536)` from the Postgres version is dropped: pgvector
-- isn't available without a running Postgres, and nothing in this slice
-- computes embeddings (that's LLM-adjacent work, also out of scope here).
-- `quality_nodes.metadata` carries the same information as plain JSON text
-- instead.

CREATE TABLE quality_nodes (
    id              TEXT PRIMARY KEY,
    project_id      TEXT,
    node_type       TEXT NOT NULL,
    external_ref    TEXT,
    name            TEXT NOT NULL,
    metadata        TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL,
    UNIQUE (project_id, node_type, external_ref)
);
CREATE INDEX idx_quality_nodes_project_type ON quality_nodes (project_id, node_type);

CREATE TABLE quality_relations (
    id              TEXT PRIMARY KEY,
    from_node_id    TEXT NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE,
    to_node_id      TEXT NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE,
    relation_type   TEXT NOT NULL,
    metadata        TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL,
    UNIQUE (from_node_id, to_node_id, relation_type)
);

-- PRD §10 / SRD §11 Risk Engine. One row per node, overwritten each time it's
-- recomputed (after every run) — `inputs` keeps the raw factors alongside the
-- score for explainability, per SRD §11 and per the PRD-critique finding
-- that risk scores need to stop being "qualitative only".
CREATE TABLE risk_assessments (
    id                      TEXT PRIMARY KEY,
    node_id                 TEXT NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE,
    score                   REAL NOT NULL,
    business_criticality    REAL,
    change_frequency        REAL,
    historical_failure_rate REAL,
    dependency_count        INTEGER,
    security_sensitivity    REAL,
    execution_cost          REAL,
    inputs                  TEXT NOT NULL DEFAULT '{}',
    computed_at             TEXT NOT NULL,
    UNIQUE (node_id)
);

CREATE TABLE test_value_assessments (
    id                          TEXT PRIMARY KEY,
    node_id                     TEXT NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE,
    value_score                 REAL NOT NULL,
    risk_coverage                REAL,
    historical_defect_detection REAL,
    business_criticality        REAL,
    change_relevance            REAL,
    execution_cost               REAL,
    flakiness                    REAL,
    computed_at                  TEXT NOT NULL,
    UNIQUE (node_id)
);

CREATE TABLE flakiness_metrics (
    id                      TEXT PRIMARY KEY,
    node_id                 TEXT NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE,
    flakiness_score         REAL NOT NULL,
    window_runs             INTEGER NOT NULL,
    window_failures         INTEGER NOT NULL,
    environment_correlation TEXT,
    computed_at             TEXT NOT NULL,
    UNIQUE (node_id)
);

CREATE TABLE failure_clusters (
    id                     TEXT PRIMARY KEY,
    project_id             TEXT,
    signature              TEXT NOT NULL,
    first_seen_at          TEXT NOT NULL,
    last_seen_at           TEXT NOT NULL,
    occurrence_count       INTEGER NOT NULL DEFAULT 1,
    root_cause_hypothesis  TEXT,
    confidence             REAL,
    reproducibility        REAL,
    review_state           TEXT NOT NULL DEFAULT 'NEEDS_REVIEW',
    UNIQUE (project_id, signature)
);

CREATE TABLE change_impacts (
    id                     TEXT PRIMARY KEY,
    change_ref             TEXT NOT NULL,
    change_type            TEXT NOT NULL,
    affected_node_id       TEXT NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE,
    selected_test_node_id  TEXT REFERENCES quality_nodes(id),
    rationale              TEXT,
    computed_at            TEXT NOT NULL
);

-- SRD §15 Evidence Architecture. `object_key` is a path under the local
-- evidence store root (see `evidence.rs`) rather than an S3 key — same
-- "immutable reference, large payload never touches the DB/LLM" contract as
-- the Postgres/MinIO design, just backed by disk instead of MinIO since
-- there's no container runtime here to run MinIO in (see docker-compose.yml
-- header comment).
CREATE TABLE evidence (
    id              TEXT PRIMARY KEY,
    run_id          TEXT REFERENCES runs(id) ON DELETE CASCADE,
    task_id         TEXT,
    node_id         TEXT REFERENCES quality_nodes(id),
    kind            TEXT NOT NULL,
    object_key      TEXT NOT NULL,
    content_type    TEXT,
    size_bytes      INTEGER,
    metadata        TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL
);
CREATE INDEX idx_evidence_run_id ON evidence (run_id);

-- PRD §21 MVP scope "bug drafting" — populated by rule-based templating in
-- `intelligence.rs` (see file header for why this isn't LLM-authored prose
-- yet). `review_state` starts NEEDS_REVIEW and only a human calling
-- POST /bug-drafts/:id/promote moves it to CONFIRMED — this is the
-- anomaly -> confirmed-defect promotion path the original PRD critique
-- flagged as undefined (PRD §14); it's now an explicit, human-gated API call
-- rather than left implicit.
CREATE TABLE bug_drafts (
    id                  TEXT PRIMARY KEY,
    failure_cluster_id  TEXT REFERENCES failure_clusters(id),
    title               TEXT NOT NULL,
    description         TEXT NOT NULL,
    severity            TEXT,
    confidence          REAL,
    review_state        TEXT NOT NULL DEFAULT 'NEEDS_REVIEW',
    created_at          TEXT NOT NULL,
    promoted_at         TEXT,
    promoted_by         TEXT
);
