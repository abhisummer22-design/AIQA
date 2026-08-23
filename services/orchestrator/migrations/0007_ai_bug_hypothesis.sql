-- Build order item 4 (22 August 2026) — ACC-018 extension: an on-demand,
-- AI-assisted root-cause hypothesis layered on top of the existing,
-- already-real, rule-based failure-cluster hypothesis
-- (`root_cause_hypothesis` — `intelligence.rs::classify_failure`,
-- deterministic, untouched by this migration or anything it enables).
--
-- Purely additive and nullable: a cluster with no AI hypothesis attempted
-- yet (the overwhelming default — this is never triggered automatically,
-- only via an explicit `POST .../findings/:id/ai-hypothesis` call) has all
-- four columns NULL, and a failed/unavailable AI attempt never writes here
-- at all (see `intelligence.rs::generate_ai_hypothesis` and
-- `store.rs::set_ai_hypothesis`) — so NULL also honestly covers "the one
-- attempt made so far failed", not just "never attempted".
--
-- Kept as separate, clearly-named columns rather than merged into
-- `root_cause_hypothesis`, per ACC-010's own standing "AI hypotheses must
-- remain distinguishable from confirmed facts" criteria — a caller can
-- always tell which hypothesis (if either) is real deterministic output
-- and which is AI-assisted, and who/what produced the latter.
ALTER TABLE failure_clusters ADD COLUMN ai_hypothesis TEXT;
ALTER TABLE failure_clusters ADD COLUMN ai_hypothesis_provider TEXT;
ALTER TABLE failure_clusters ADD COLUMN ai_hypothesis_model TEXT;
ALTER TABLE failure_clusters ADD COLUMN ai_hypothesis_generated_at TEXT;
