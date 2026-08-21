/**
 * FE-11 — Custom Testing Workflows: truthful "not implemented" page
 * (Case C).
 *
 * The previous version rendered a fabricated modular test-suite builder:
 * a "create suite" form for Visual Regression/Load & Stress/Chaos
 * Resilience/Synthetic E2E categories, all persisted only to
 * `AIQAContext`'s local, in-memory `customSuites` array, with "Run Suite"
 * only flipping local status client-side.
 *
 * Re-verified against the real backend route table at the start of this
 * sprint: there is no custom-test-suite entity, no test category/
 * classification concept, and no visual-diff/load-generation/chaos-
 * injection endpoint anywhere in the backend — a page named "Custom
 * Testing" is not evidence such a capability exists (brief §9). The
 * backend's real, generic unit of work is a `Task` inside an `Execution`
 * (see Execution Hub) — it has no notion of these four specialized
 * categories.
 *
 * Per this sprint's Case C rule: replace the fabricated suite builder with
 * a truthful "not implemented" state rather than inventing a fallback
 * suite-category model.
 */

import React from 'react';
import { useNavigate } from 'react-router-dom';
import { Sliders } from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';

export const CustomTestingPage: React.FC = () => {
  const navigate = useNavigate();

  return (
    <div id="custom-testing-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Custom Testing Workflows"
        subtitle="Visual regression, load & stress, chaos resilience, and synthetic E2E suite builder"
        badge="NOT IMPLEMENTED"
        icon={Sliders}
      />

      <EmptyState
        icon={Sliders}
        badge="NO BACKEND CAPABILITY"
        title="Custom test suites are not implemented"
        description="The backend has no custom-test-suite entity or category-specific dispatch endpoint (visual diff, load generation, chaos injection) — this page previously showed a fabricated suite builder instead of a real feature. The backend's real unit of work is a Task inside an Execution, scheduled through the Execution Hub."
        actionLabel="Go to Execution Hub"
        onAction={() => navigate('/execution')}
        codeHint="No corresponding route exists in services/orchestrator/src/api.rs"
      />
    </div>
  );
};
