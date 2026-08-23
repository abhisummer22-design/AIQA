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
 * Per that sprint's Case C rule, the fabricated UI was replaced with a
 * truthful "not implemented" state pointing at the real capability that
 * exists instead — no fallback DAG/plan model invented to make the page
 * "feel" functional.
 *
 * **Copy updated 22 August 2026** (a second, later, copy-only pass — no
 * backend/frontend behavior changed): `AIQA_PRD_FRD_SRD_v6.1_SCOPE_
 * CORRECTIONS.md` §2 has since recorded this capability (AI-assisted
 * cross-agent routing/prioritization/coordination, ACC-006) as a direct
 * product-owner decision — real, intended future scope, deliberately
 * *deferred*, not an unexplained gap and not descoped. "Not implemented"
 * was accurate but generic; it read the same as a page nobody had ever
 * decided about. The copy below now says which of those it actually is,
 * and names the real, recorded revisit condition, rather than leaving a
 * reader to guess. Still no orchestration-plan entity, DAG/dependency
 * model, specialized-agent worker pool, or risk-scoring capability exists
 * in the backend today — that underlying fact is unchanged, only how
 * plainly it's explained.
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
        badge="DEFERRED — REAL FUTURE SCOPE"
        icon={Workflow}
      />

      <EmptyState
        icon={Workflow}
        badge="DEFERRED, NOT DESCOPED"
        title="Orchestration planning is real, deferred scope — not committed yet"
        description="AI-assisted cross-agent routing, prioritization, and coordination remains real, intended product scope — deliberately deferred, not rejected, per a direct product-owner decision (AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md §2, 22 August 2026). Revisit condition: no fixed trigger — revisit if product direction shifts toward autonomous routing; today's DAG-scheduler orchestrator, run through the Execution Hub, is the committed model. No orchestration-plan entity, DAG/dependency model, or specialized-agent worker pool exists in the backend today."
        actionLabel="Go to Execution Hub"
        onAction={() => navigate('/execution')}
        codeHint="ACC-006 — AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md §2"
      />
    </div>
  );
};
