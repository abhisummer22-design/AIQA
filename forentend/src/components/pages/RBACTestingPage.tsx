/**
 * FE-11 — RBAC Security Testing Matrix: truthful "not implemented" page
 * (Case C).
 *
 * The previous version rendered fabricated `RBACScenario`/`RBACMatrixRule`
 * demo data — an invented Role → Permission → Resource → Action → Expected
 * Access → Actual Access → Result pipeline with no real evaluation behind
 * it (`evaluateRBACMatrix` just flipped local state to a hardcoded
 * result), plus a form that added more fake rows to that same fabricated
 * matrix.
 *
 * Re-verified against the real backend route table at the start of this
 * sprint: there is no role-permission-matrix entity and no endpoint that
 * evaluates "does role X get access Y on resource Z" — the backend's real
 * RBAC is enforced inline, per-request, by `auth::require_role` on each
 * existing route (e.g. `review_finding_scoped`/`get_audit_log_scoped`
 * genuinely require `Role::Operator`, and this frontend's own real pages
 * surface those real 401/403 responses when a key's role doesn't qualify
 * — see `StateViews.tsx`'s `ForbiddenState`). There is no dedicated
 * "run an authorization audit matrix" capability to migrate this page
 * onto (brief §9: a page's name is not evidence a backend capability
 * exists).
 *
 * Per this sprint's Case C rule: replace the fabricated matrix with a
 * truthful "not implemented" state rather than inventing a fallback
 * evaluation engine.
 */

import React from 'react';
import { ShieldCheck } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';

export const RBACTestingPage: React.FC = () => {
  const navigate = useNavigate();

  return (
    <div id="rbac-testing-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="RBAC Security Testing Matrix"
        subtitle="Role-permission authorization boundary auditing"
        badge="NOT IMPLEMENTED"
        icon={ShieldCheck}
      />

      <EmptyState
        icon={ShieldCheck}
        badge="NO BACKEND CAPABILITY"
        title="RBAC matrix testing is not implemented"
        description="The backend has no role-permission-matrix entity or endpoint that evaluates authorization scenarios — this page previously showed a fabricated audit matrix instead of a real feature. The backend's real RBAC is enforced inline on each protected route (Operator/Admin-gated actions genuinely return 401/403 to an under-privileged key); the closest real evidence of that enforcement is the project's Audit Trail."
        actionLabel="Go to Audit Trail"
        onAction={() => navigate('/audit')}
        codeHint="No role-matrix route exists in services/orchestrator/src/api.rs — see auth::require_role call sites for real, per-route enforcement"
      />
    </div>
  );
};
