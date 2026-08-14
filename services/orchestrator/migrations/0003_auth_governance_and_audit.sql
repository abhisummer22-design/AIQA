-- Auth/authz/multi-tenancy + AI-governance/policy-engine + audit log — all
-- flagged as "Completely absent" / "Not implemented" in the prior session's
-- build report (Section 5) and in the original PRD critique ("no
-- auth/roles/multi-tenancy anywhere ... needs resolving before the data
-- model is finalized"). See auth.rs / policy.rs.

-- API keys are opaque random tokens; only a SHA-256 hash is ever stored
-- (see auth.rs::hash_key) — the plaintext key is shown exactly once, at
-- creation time, same as GitHub/Stripe-style tokens.
CREATE TABLE api_keys (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    key_hash    TEXT NOT NULL UNIQUE,
    role        TEXT NOT NULL,          -- 'admin' | 'operator' | 'viewer'
    project_id  TEXT,                   -- NULL = admin/global scope, sees every project
    created_at  TEXT NOT NULL,
    revoked_at  TEXT
);

-- Run-level governance columns: a run whose plan trips a policy rule
-- (policy.rs) lands in PENDING_APPROVAL instead of being scheduled, and sits
-- there until an operator/admin calls POST /runs/:id/approve or /reject —
-- the human-approval gate the PRD's "governed autonomous execution" (and
-- the original critique) presupposed but nothing implemented.
ALTER TABLE runs ADD COLUMN requested_by TEXT;
ALTER TABLE runs ADD COLUMN policy_notes TEXT;
ALTER TABLE runs ADD COLUMN approved_by TEXT;
ALTER TABLE runs ADD COLUMN approved_at TEXT;

-- Append-only audit log: every authenticated mutation (run creation,
-- approve/reject, bug-draft promotion, key issuance) plus auth failures.
-- Never updated or deleted from the API surface.
CREATE TABLE audit_log (
    id          TEXT PRIMARY KEY,
    occurred_at TEXT NOT NULL,
    actor       TEXT,                   -- api key name, or NULL for unauthenticated attempts
    role        TEXT,
    project_id  TEXT,
    action      TEXT NOT NULL,          -- 'run.create' | 'run.approve' | 'run.reject' | 'auth.fail' | ...
    target      TEXT,                   -- run id / bug draft id / key id, as applicable
    detail      TEXT,
    outcome     TEXT NOT NULL           -- 'ok' | 'denied' | 'error'
);
CREATE INDEX idx_audit_log_occurred_at ON audit_log (occurred_at);
