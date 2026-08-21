/**
 * FE-11 — Test Case Validator: truthful "not implemented" page (Case C).
 *
 * The previous version rendered 5 hardcoded `ValidationRuleCheck` records
 * with an invented traceability/quality-score/duplicate-similarity/risk-
 * coverage model, an "AI Auto-Fix" button that rewrote those same
 * hardcoded records client-side, and top-line metrics (coverage %, mean
 * quality index, duplicate cluster count) computed only from that fake
 * data.
 *
 * Re-verified against the real backend route table at the start of this
 * sprint: there is no requirement-traceability entity, no test-case
 * quality-scoring model, no duplicate/semantic-similarity detection, and
 * no requirement-linkage concept anywhere in the backend — a page named
 * "Validator" is not evidence such a capability exists (brief §9). The
 * closest real, adjacent capability is the Findings page's real recurring-
 * failure detection (`FailureCluster`/`Finding`), which is a different
 * thing (post-hoc failure clustering, not pre-execution test validation).
 *
 * Per this sprint's Case C rule: replace the fabricated UI with a truthful
 * "not implemented" state and point at the real capabilities that exist
 * instead, rather than inventing a fallback scoring model.
 */

import React from 'react';
import { useNavigate } from 'react-router-dom';
import { CheckCheck, Sparkles } from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';

export const TestCaseValidatorPage: React.FC = () => {
  const navigate = useNavigate();

  return (
    <div id="test-case-validator-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Test Case Validator"
        subtitle="Requirement traceability, quality scoring, and duplicate detection prior to execution"
        badge="NOT IMPLEMENTED"
        icon={CheckCheck}
      />

      <EmptyState
        icon={CheckCheck}
        badge="NO BACKEND CAPABILITY"
        title="Test case validation is not implemented"
        description="The backend has no requirement-traceability, quality-scoring, or duplicate-detection entity today — this page previously showed fabricated demo data instead of a real feature. The closest real capability is Findings, which surfaces recurring failure signatures the orchestrator has actually detected from executed tasks (a different, post-hoc analysis, not pre-execution validation)."
        actionLabel="Go to Findings"
        onAction={() => navigate('/reports')}
        secondaryActionLabel="Go to Test Generator"
        onSecondaryAction={() => navigate('/test-generator')}
        codeHint="No corresponding route exists in services/orchestrator/src/api.rs"
      />
    </div>
  );
};
