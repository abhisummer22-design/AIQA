-- Target-schema mirror of services/orchestrator/migrations/0007_ai_bug_hypothesis.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale). Phase 3 gap-closing.
--
-- An on-demand, AI-assisted root-cause hypothesis layered on top of the
-- existing, already-real, rule-based failure-cluster hypothesis
-- (`root_cause_hypothesis`, defined in 002_quality_graph_and_intelligence.sql).
-- Purely additive and nullable: a cluster with no AI hypothesis attempted
-- yet has all four columns NULL, and a failed/unavailable AI attempt never
-- writes here at all — NULL honestly covers "never attempted" and "the one
-- attempt made so far failed" alike.
ALTER TABLE failure_clusters ADD COLUMN ai_hypothesis TEXT;
ALTER TABLE failure_clusters ADD COLUMN ai_hypothesis_provider TEXT;
ALTER TABLE failure_clusters ADD COLUMN ai_hypothesis_model TEXT;
ALTER TABLE failure_clusters ADD COLUMN ai_hypothesis_generated_at TIMESTAMPTZ;
