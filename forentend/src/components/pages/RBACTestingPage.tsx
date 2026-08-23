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
 * Per that sprint's Case C rule, the fabricated matrix was replaced with a
 * truthful "not implemented" state rather than a fallback evaluation
 * engine.
 *
 * **Copy updated 22 August 2026** (a second, later, copy-only pass — no
 * backend/frontend behavior changed): `AIQA_PRD_FRD_SRD_v6.1_SCOPE_
 * CORRECTIONS.md` §2 has since recorded the dedicated scenario-matrix
 * *tool* (ACC-011) as a direct product-owner decision — real, intended
 * future scope, deliberately *deferred*, not an unexplained gap and not
 * descoped. Sharpened distinction, carried over from this file's original
 * doc comment above and now also stated in the rendered copy: only the
 * dedicated evaluation *tool* is deferred — real RBAC *enforcement* is a
 * different thing, already fully committed and already shipped
 * (`auth::require_role` on every protected route). "Not implemented" was
 * accurate but generic and did not make that distinction visible on the
 * page itself; the copy below now does. Still no role-permission-matrix
 * entity or evaluation endpoint exists in the backend today — that
 * underlying fact is unchanged, only how plainly it's explained.
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
        badge="DEFERRED — REAL FUTURE SCOPE"
        icon={ShieldCheck}
      />

      <EmptyState
        icon={ShieldCheck}
        badge="DEFERRED, NOT DESCOPED"
        title="The RBAC scenario-matrix tool is real, deferred scope — enforcement itself already ships"
        description="A dedicated systematic role×permission×action evaluation tool remains real, intended product scope — deliberately deferred, not rejected, per a direct product-owner decision (AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md §2, 22 August 2026). Revisit condition: no fixed trigger — real RBAC enforcement remains fully committed and already shipped (auth::require_role on every protected route); only this dedicated evaluation tool is deferred. No role-permission-matrix entity or evaluation endpoint exists in the backend today; the closest real evidence of enforcement is the project's Audit Trail."
        actionLabel="Go to Audit Trail"
        onAction={() => navigate('/audit')}
        codeHint="ACC-011 (scenario-matrix tool) — AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md §2"
      />
    </div>
  );
};
