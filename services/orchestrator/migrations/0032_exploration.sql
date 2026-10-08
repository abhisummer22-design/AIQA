-- Phase 5 (AI Exploratory Testing Agent) — persistence for exploration sessions and
-- the findings they produce.
--
-- WHY THIS EXISTS
--
-- Exploration is the first feature in this codebase whose *output is a judgement about
-- the application* rather than a measurement of it. Everything else -- telemetry, failure
-- clusters, change impacts, risk -- records something that happened. Exploration records
-- something an agent believes it saw.
--
-- That difference is exactly why it needs to be persisted rather than returned and
-- forgotten:
--
-- 1. DUPLICATE SUPPRESSION MUST SURVIVE THE PROCESS. A defect reached by two different
--    exploration paths is one defect. Suppressing it requires remembering what has already
--    been reported, which means storage. An in-memory set would make the second session
--    rediscover and re-report every finding the first one made.
--
-- 2. THE CLASSIFICATION MUST BE AUDITABLE. `CONFIRMED_DEFECT` is a claim about the
--    application. Whoever reads it later needs the evidence key, the action sequence and
--    the stated basis -- not a boolean that some agent once returned.
--
-- 3. EVIDENCE IS IMMUTABLE AND EXTERNAL. Following `evidence.rs`, the artifact bytes live
--    in the evidence store addressed by `object_key`; the row holds the key. Evidence is
--    never inlined into a database row, and never handed to a model.
--
-- THIS IS PURELY ADDITIVE
--
-- Two new tables. Nothing is altered, dropped or rewritten, so this is not a destructive
-- migration and does not trip the plan's stop conditions. There is no pre-existing data
-- to lose, because exploration has never run.
--
-- WHY `project_id` IS A COLUMN AND NOT A JOIN
--
-- Same reasoning as Phase 3's migration 0031, applied to the one table where a forgotten
-- `WHERE` is a cross-tenant leak rather than an empty result. `exploration_findings` is
-- read by project constantly and by nothing else; ownership is stated, not inferred.

CREATE TABLE IF NOT EXISTS exploration_sessions (
    id              TEXT PRIMARY KEY,
    project_id      TEXT NOT NULL,
    -- What was explored: the target base URL or descriptor handed to the agent.
    target          TEXT NOT NULL,
    -- The budget the caller asked for, stored so a reader can see the ceiling that was
    -- in force rather than inferring it from what happened.
    max_actions             INTEGER NOT NULL,
    max_state_transitions   INTEGER NOT NULL,
    max_findings            INTEGER NOT NULL,
    max_pages               INTEGER NOT NULL,
    -- The agent limits actually applied (turns, tool calls, observation bytes, timeout).
    max_turns               INTEGER NOT NULL,
    max_tool_calls          INTEGER NOT NULL,
    -- RUNNING | COMPLETED | MAX_BUDGET_REACHED | CANCELLED | FAILED
    status          TEXT NOT NULL,
    -- What the agent did, as counted. `actions_taken` is the enforced figure; the rest are
    -- reported so a truncated run is legible rather than merely short.
    actions_taken          INTEGER NOT NULL DEFAULT 0,
    state_transitions_taken INTEGER NOT NULL DEFAULT 0,
    pages_visited          INTEGER NOT NULL DEFAULT 0,
    findings_recorded      INTEGER NOT NULL DEFAULT 0,
    findings_suppressed    INTEGER NOT NULL DEFAULT 0,
    -- Why the run ended, verbatim from the agent runtime. Never empty: a run that stopped
    -- without saying why is the one a reader cannot trust.
    stop_reason     TEXT NOT NULL,
    error           TEXT,
    started_at      TEXT NOT NULL,
    finished_at     TEXT
);

-- Reading a project's exploration history, newest first. Covers the common query without a
-- scan once a project has accumulated sessions.
CREATE INDEX IF NOT EXISTS idx_exploration_sessions_project
    ON exploration_sessions(project_id, started_at DESC);

CREATE TABLE IF NOT EXISTS exploration_findings (
    id              TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL REFERENCES exploration_sessions(id) ON DELETE CASCADE,
    project_id      TEXT NOT NULL,
    -- OBSERVED_ANOMALY | POTENTIAL_DEFECT | CONFIRMED_DEFECT | INSUFFICIENT_EVIDENCE
    classification  TEXT NOT NULL,
    -- The catalogue entry that produced it, so a reader can see which exploratory
    -- behaviour was being tested rather than only that something went wrong.
    behaviour       TEXT NOT NULL,
    route           TEXT NOT NULL,
    -- The real fingerprint of the failure the recorded execution produced, computed by
    -- `adaptive_intelligence::canonical_failure_fingerprint`. NULL for a finding with no
    -- recorded failure behind it -- which is most `INSUFFICIENT_EVIDENCE` rows, and is why
    -- those rows are deliberately NOT deduplicated (see the unique index below).
    failure_fingerprint TEXT,
    -- Key into the evidence store. NULL only when nothing was captured, which the
    -- classification already reflects.
    evidence_object_key  TEXT,
    observed       TEXT NOT NULL,
    -- What the application was expected to do. NULL when exploration had no stated
    -- expectation -- an observed anomaly with no requirement behind it is an anomaly, not
    -- a defect, and the row says so instead of inventing an expectation.
    expected               TEXT,
    violated_requirement   TEXT,
    -- The exact `Action` sequence that was executed, as the real Action AST serialized.
    -- Stored so a finding can be replayed rather than taken on trust.
    action_sequence TEXT NOT NULL,
    -- Application state at observation time (role, url, visible elements), as JSON.
    app_state       TEXT NOT NULL,
    -- 0.0-1.0. Never asserted above what the evidence supports; see
    -- `ExplorationFinding::classify` in `agents/exploration.rs`.
    confidence      REAL NOT NULL,
    -- One clause naming what established the classification. This is the field that makes
    -- the row auditable: it states the evidence, not the verdict.
    basis           TEXT NOT NULL,
    -- The related historical failure or bug draft, when Phase 2's fingerprint matched one.
    related_failure TEXT,
    observed_at     TEXT NOT NULL
);

-- Reading one project's findings, newest first, optionally filtered by severity. Covers
-- the common query without a scan.
CREATE INDEX IF NOT EXISTS idx_exploration_findings_project
    ON exploration_findings(project_id, classification, observed_at DESC);

-- Reading one session's findings in the order they were observed.
CREATE INDEX IF NOT EXISTS idx_exploration_findings_session
    ON exploration_findings(session_id, observed_at);

-- WHY THIS UNIQUE INDEX IS THE DUPLICATE SUPPRESSION MECHANISM
--
-- One reported defect per project per underlying failure. The same failure reached through
-- a different route, a different action order, or a different session produces the same
-- `canonical_failure_fingerprint`, because the fingerprint is computed from the failure
-- category, the masked reason, the selector and the endpoint -- not from the path taken to
-- get there. So this index makes "the same underlying defect" a property of the schema
-- rather than of application code remembering what it has seen.
--
-- The writer relies on `ON CONFLICT DO NOTHING` and then re-reads to learn whether the row
-- it tried to insert was already there. That makes the second report a recorded
-- *suppression* rather than a silent duplicate, which is the difference between a system
-- that deduplicates and one that loses information.
--
-- NULLs are deliberately not folded. SQLite treats NULLs as distinct in a unique index, and
-- that is the behaviour wanted here: an `INSUFFICIENT_EVIDENCE` row has no fingerprint, is
-- not a claim about the application, and must not stop the same inconclusive observation
-- being recorded in a later session. `COALESCE(fingerprint, ...)` here would suppress real
-- observations, which is the opposite of the requirement.
CREATE UNIQUE INDEX IF NOT EXISTS uq_exploration_findings_defect
    ON exploration_findings(project_id, failure_fingerprint)
    WHERE failure_fingerprint IS NOT NULL;
