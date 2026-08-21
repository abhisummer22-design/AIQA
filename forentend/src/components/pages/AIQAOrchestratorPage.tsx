/**
 * FE-11 — AIQA Orchestrator: truthful "not implemented" page (Case C).
 *
 * The previous version of this page rendered a fully fabricated
 * orchestration-planning UI: hardcoded `OrchestratorPlan` objects
 * (`PLAN-901`/`PLAN-902`), a fake DAG visualizer with invented dependency
 * graphs, fake "specialized agent" worker-pool assignments, and a
 * risk/change/test-value scoring model with no real computation behind it.
 * "Dispatch Selected Plan" called the real `triggerNewRun` only to attach
 * a fabricated plan name to it — a real execution with a fictional
 * justification, which is worse than an honest unsupported state.
 *
 * Re-verified against the real backend route table at the start of this
 * sprint (`services/orchestrator/src/api.rs`'s full route list): there is
 * no orchestration-plan entity, no DAG/dependency model, no specialized-
 * agent/worker-pool concept, and no risk/change/test-value scoring anywhere
 * in the backend. A page named "Orchestrator" is not evidence such a
 * capability exists (this sprint's own brief §9) — the backend's real
 * orchestration is the execution engine itself (`engine.rs`), already
 * fully exposed through the real Execution Hub (`/execution`).
 *
 * Per this sprint's Case C rule: replace the fabricated UI with a truthful
 * "not implemented" state (existing `EmptyState` component, no new design
 * language), point at the real capability that exists instead, and do not
 * invent a fallback DAG/plan model to make the page "feel" functional.
 */

import React from 'react';
import { useNavigate } from 'react-router-dom';
import { Workflow, PlayCircle } from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';

export const AIQAOrchestratorPage: React.FC = () => {
  const navigate = useNavigate();

  return (
    <div id="aiqa-orchestrator-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="AIQA Orchestrator"
        subtitle="Multi-agent orchestration planning and DAG-based dependency routing"
        badge="NOT IMPLEMENTED"
        icon={Workflow}
      />

      <EmptyState
        icon={Workflow}
        badge="NO BACKEND CAPABILITY"
        title="Orchestration planning is not implemented"
        description="The AIQA Orchestrator backend has no orchestration-plan entity, DAG/dependency model, specialized-agent worker pool, or risk-scoring capability today — this page previously showed fabricated demo data instead of a real feature. Real test execution is scheduled and run through the Execution Hub."
        actionLabel="Go to Execution Hub"
        onAction={() => navigate('/execution')}
        codeHint="No corresponding route exists in services/orchestrator/src/api.rs"
      />
    </div>
  );
};
