/**
 * Test Case Validator — honest redirect to the real capability (ACC-005,
 * same shape as ACC-012's `APITestingPage` fix).
 *
 * FE-11 (Case C) replaced this page's original fabricated UI (5 hardcoded
 * `ValidationRuleCheck` records, an invented traceability/quality-score/
 * duplicate-similarity/risk-coverage model, a client-side "AI Auto-Fix"
 * button) with a "not implemented" state. That was correct at the time —
 * no such entity existed anywhere. It is no longer accurate: AUD-005 (a
 * later unit) added a real, tested validate → approve/reject → handoff
 * lifecycle (`POST .../test-cases/:id/validate|approve|reject|handoff`,
 * `store.rs`, 19 passing `aud005_*` tests), and it is already wired up and
 * live in `TestCaseGeneratorPage.tsx` — the "Validate Test Case"/"Approve
 * Test Case"/"Reject" buttons in `GeneratedTestCasesView.tsx`, gated on
 * each case's real backend status.
 *
 * Investigated fresh this session before touching this file, per the
 * standing "verify before building" rule: the real capability was
 * confirmed already fully wired elsewhere, so this is a copy/routing fix,
 * not a new page — the same shape ACC-012 just was.
 *
 * What this does NOT claim: the real backend validator (`api.rs::
 * validate_test_case_scoped`) is a **structural** check only — title,
 * description, steps, and expected_result are non-empty and every step has
 * an action and an expected result. It does not implement the richer
 * semantic categories the Acceptance Matrix's own ACC-005 criteria
 * describe (duplicate/overlap detection, quality-benchmark scoring,
 * business-logic/RBAC/security coverage, risk assessment) — those remain
 * genuinely unimplemented, not merely relocated. This page says so
 * honestly rather than implying the redirect closes the full criteria.
 */

import React from 'react';
import { useNavigate } from 'react-router-dom';
import { CheckCheck } from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';

export const TestCaseValidatorPage: React.FC = () => {
  const navigate = useNavigate();

  return (
    <div id="test-case-validator-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Test Case Validator"
        subtitle="Real validation runs on the Test Generator page, not a separate screen here"
        badge="VIA TEST GENERATOR"
        icon={CheckCheck}
      />

      <EmptyState
        icon={CheckCheck}
        badge="IMPLEMENTED ELSEWHERE"
        title="Test case validation runs on the Test Generator page"
        description="This page has no dedicated validator UI of its own. Real, structural validation — plus approve, reject, and execution handoff — happens directly on a saved test case's own card on the Test Generator page: open a project's generated/saved test cases there and use Validate / Approve / Reject. What real validation does NOT yet check: duplicate/overlap detection, quality-benchmark scoring, or business-logic/RBAC/security/risk coverage — only structural completeness (title, steps, expected result) is verified today."
        actionLabel="Go to Test Generator"
        onAction={() => navigate('/test-generator')}
        secondaryActionLabel="Go to Findings"
        onSecondaryAction={() => navigate('/reports')}
        codeHint="POST /api/v1/projects/:id/test-cases/:id/validate — services/orchestrator/src/api.rs::validate_test_case_scoped"
      />
    </div>
  );
};
