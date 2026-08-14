-- Quality graph + intelligence-layer entities (PRD §8-19, SRD §10-14, §19).
-- Scaffolding only: nothing in services/orchestrator populates these tables
-- yet (no AI planning, risk engine, failure/flakiness intelligence, or
-- change intelligence is implemented in this slice — see README/critique).
-- pgvector is used for semantic document retrieval only, not as the primary
-- knowledge representation (SRD §10).

CREATE EXTENSION IF NOT EXISTS vector;

-- SRD §10 Quality Graph Storage: relational entities + explicit relationship
-- tables. `quality_nodes` covers requirements/features/components/screens/
-- APIs/tables/tests/executions/findings/bugs/changes/environments (PRD §8)
-- as one polymorphic node table; `quality_relations` is the SRD §19 entity.
CREATE TABLE quality_nodes (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id      UUID,
    node_type       TEXT NOT NULL,   -- 'requirement' | 'feature' | 'component' | 'screen'
                                      -- | 'api' | 'table' | 'test' | 'execution' | 'finding'
                                      -- | 'bug' | 'change' | 'environment'
    external_ref    TEXT,            -- id in the source system (Jira key, file path, ...)
    name            TEXT NOT NULL,
    metadata        JSONB NOT NULL DEFAULT '{}',
    embedding       vector(1536),    -- semantic retrieval only (SRD §10)
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_quality_nodes_project_type ON quality_nodes (project_id, node_type);

CREATE TABLE quality_relations (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    from_node_id    UUID NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE,
    to_node_id      UUID NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE,
    relation_type   TEXT NOT NULL,   -- 'covers' | 'depends_on' | 'implements' | 'impacts' | ...
    metadata        JSONB NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (from_node_id, to_node_id, relation_type)
);

-- PRD §10 / SRD §11 Risk Engine. Inputs stored alongside the score for
-- explainability, per SRD §11.
CREATE TABLE risk_assessments (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id             UUID NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE,
    score               NUMERIC NOT NULL,
    business_criticality NUMERIC,
    change_frequency    NUMERIC,
    historical_failure_rate NUMERIC,
    dependency_count    INTEGER,
    security_sensitivity NUMERIC,
    execution_cost      NUMERIC,
    inputs              JSONB NOT NULL DEFAULT '{}',
    computed_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- PRD §12 Test Value Intelligence.
CREATE TABLE test_value_assessments (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id             UUID NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE,
    value_score         NUMERIC NOT NULL,
    risk_coverage       NUMERIC,
    historical_defect_detection NUMERIC,
    business_criticality NUMERIC,
    change_relevance    NUMERIC,
    execution_cost      NUMERIC,
    flakiness           NUMERIC,
    computed_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- PRD §17 / SRD §14 Flakiness Engine.
CREATE TABLE flakiness_metrics (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id             UUID NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE,
    flakiness_score     NUMERIC NOT NULL,
    window_runs         INTEGER NOT NULL,
    window_failures     INTEGER NOT NULL,
    environment_correlation JSONB,
    computed_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE failure_clusters (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    project_id          UUID,
    signature            TEXT NOT NULL,   -- normalized fingerprint of the failure
    first_seen_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_seen_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    occurrence_count     INTEGER NOT NULL DEFAULT 1,
    root_cause_hypothesis TEXT,
    confidence            NUMERIC,        -- PRD §15 Confidence & Evidence
    reproducibility        NUMERIC,
    review_state           TEXT NOT NULL DEFAULT 'NEEDS_REVIEW' -- see PRD §14 Anomaly vs Defect
);

-- PRD §11 / SRD §12 Change Intelligence.
CREATE TABLE change_impacts (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    change_ref          TEXT NOT NULL,    -- commit sha / config version / schema migration id
    change_type         TEXT NOT NULL,    -- 'code' | 'config' | 'api' | 'schema'
    affected_node_id    UUID NOT NULL REFERENCES quality_nodes(id) ON DELETE CASCADE,
    selected_test_node_id UUID REFERENCES quality_nodes(id),
    rationale            TEXT,            -- FRD §11: "explain why each selected test is relevant"
    computed_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- SRD §15 Evidence Architecture: immutable references to object storage +
-- metadata here; large files never pass through Postgres or the LLM directly.
CREATE TABLE evidence (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id          UUID REFERENCES runs(id) ON DELETE CASCADE,
    task_id         TEXT,
    node_id         UUID REFERENCES quality_nodes(id),
    kind            TEXT NOT NULL,   -- 'screenshot' | 'video' | 'trace' | 'log' | 'http' | 'ocr' | 'perf' | 'security'
    object_key      TEXT NOT NULL,   -- S3-compatible object storage key
    content_type    TEXT,
    size_bytes      BIGINT,
    metadata        JSONB NOT NULL DEFAULT '{}',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- PRD §21 MVP scope "bug drafting". No issue-tracker integration in this
-- slice (PRD §22 defers it) — drafts live here only.
CREATE TABLE bug_drafts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    failure_cluster_id UUID REFERENCES failure_clusters(id),
    title           TEXT NOT NULL,
    description     TEXT NOT NULL,
    severity        TEXT,
    confidence      NUMERIC,
    review_state    TEXT NOT NULL DEFAULT 'NEEDS_REVIEW',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
