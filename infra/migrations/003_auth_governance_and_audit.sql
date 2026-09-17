-- Target-schema mirror of services/orchestrator/migrations/0003_auth_governance_and_audit.sql
-- (the real, running SQLite migration — see that file's own comment for the
-- full rationale). Phase 3 gap-closing: infra/migrations/ had drifted to
-- cover only the first 2 of 17 real migrations; this is one of 15 new files
-- closing that gap, still NOT wired into services/orchestrator (unchanged
-- from 001/002's own disclosed "not yet wired" status — see 001's own
-- header comment).
--
-- Auth/authz/multi-tenancy + AI-governance/policy-engine + audit log.

-- API keys are opaque random tokens; only a SHA-256 hash is ever stored
-- (see auth.rs::hash_key) — the plaintext key is shown exactly once, at
-- creation time, same as GitHub/Stripe-style tokens.
CREATE TABLE api_keys (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    key_hash    TEXT NOT NULL UNIQUE,
    role        TEXT NOT NULL,          -- 'admin' | 'operator' | 'viewer'
    project_id  UUID,                   -- NULL = admin/global scope, sees every project
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at  TIMESTAMPTZ
);

-- Run-level governance columns: a run whose plan trips a policy rule
-- (policy.rs) lands in PENDING_APPROVAL instead of being scheduled, and sits
-- there until an operator/admin calls POST /runs/:id/approve or /reject.
ALTER TABLE runs ADD COLUMN requested_by TEXT;
ALTER TABLE runs ADD COLUMN policy_notes TEXT;
ALTER TABLE runs ADD COLUMN approved_by TEXT;
ALTER TABLE runs ADD COLUMN approved_at TIMESTAMPTZ;

-- Append-only audit log: every authenticated mutation (run creation,
-- approve/reject, bug-draft promotion, key issuance) plus auth failures.
-- Never updated or deleted from the API surface. `target` is deliberately
-- TEXT, not UUID: it names whichever entity (run/bug-draft/key/...) the
-- action concerned, a real polymorphic reference with no single FK target.
CREATE TABLE audit_log (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    actor       TEXT,                   -- api key name, or NULL for unauthenticated attempts
    role        TEXT,
    project_id  UUID,
    action      TEXT NOT NULL,          -- 'run.create' | 'run.approve' | 'run.reject' | 'auth.fail' | ...
    target      TEXT,                   -- run id / bug draft id / key id, as applicable
    detail      TEXT,
    outcome     TEXT NOT NULL           -- 'ok' | 'denied' | 'error'
);
CREATE INDEX idx_audit_log_occurred_at ON audit_log (occurred_at);
