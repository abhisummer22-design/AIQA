-- 0030_failure_cluster_fingerprint.sql
-- Phase 6 (QA Intelligence Memory): make a failure cluster correlatable to the
-- telemetry that produced it.
--
-- Why this migration exists:
--
-- Two different identifier schemes were in play for "the same failure", and nothing
-- joined them:
--
--   failure_clusters.signature            = normalize_signature(task.reason)
--                                             -> human text, e.g.
--                                                "request post /payment returned #"
--   execution_telemetry.failure_fingerprint = compute_telemetry_fingerprint(
--                                                 category, masked_reason, selector)
--                                             -> 64-char SHA-256 hex
--
-- The quality graph links a cluster's *node* by the telemetry fingerprint
-- (`upsert_quality_node(pid, "failure_cluster", fingerprint, ...)` in
-- adaptive_intelligence::record_execution_telemetry), while `failure_clusters` itself
-- is keyed by the normalized text. So a cluster and its executions could not be
-- joined by any stored value.
--
-- The consequence was measured, not assumed: `qa_memory::QaMemoryReader::bug_history`
-- reported an **empty** `related_test_cases` for every defect. "This recurring defect
-- touched no tests" is the most misleading answer the system can give, because the
-- test that produced it is right there.
--
-- This column closes the gap using the *existing* computation rather than a new one:
-- `record_failure` already holds the task reason, and
-- `ExecutionTelemetry::from_task_state` already derives the fingerprint from that same
-- reason via `sanitize_telemetry_text` -> `categorize_failure` -> `mask_dynamic_tokens`
-- -> `compute_telemetry_fingerprint`. Writing that same value here makes the two rows
-- joinable by equality.
--
-- Safety: a nullable column. Historical clusters keep NULL and stay reportable — the
-- retrieval layer falls back to the `fails_with` graph for those rather than inventing
-- a link. No existing row is dropped or rewritten.

ALTER TABLE failure_clusters ADD COLUMN failure_fingerprint TEXT;

-- Serves the one lookup that joins a cluster to its executions.
CREATE INDEX IF NOT EXISTS idx_clusters_fingerprint
    ON failure_clusters(project_id, failure_fingerprint);