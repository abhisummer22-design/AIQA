/**
 * FE-11 — Feature Map / System Call Flow: truthful "not implemented" page
 * (Case C).
 *
 * The previous version rendered a fully fabricated microservice topology:
 * 6 hardcoded `FeatureMapNode` records (invented services like "Payment
 * Ingestion Engine"/"Double-Entry Ledger Service" with fake health/
 * coverage scores) and 3 hardcoded `SystemCallFlow` traces with a
 * "Simulate Call Flow" button that just stepped through them on a timer —
 * none of it reflects this actual application's real architecture (a
 * single Rust/Axum orchestrator plus this React frontend), let alone a
 * microservice mesh.
 *
 * Re-verified against the real backend route table at the start of this
 * sprint: there is no service-topology entity, no dependency-graph model,
 * and no call-flow-tracing endpoint anywhere in the backend — a page named
 * "Feature Map" is not evidence such a capability exists (brief §9).
 *
 * Per that sprint's Case C rule, the fabricated topology/trace viewer was
 * replaced with a truthful "not implemented" state rather than a fallback
 * architecture diagram.
 *
 * **Copy updated 22 August 2026** (a second, later, copy-only pass — no
 * backend/frontend behavior changed): `AIQA_PRD_FRD_SRD_v6.1_SCOPE_
 * CORRECTIONS.md` §2 has since recorded the visual quality-graph/
 * traceability UI (ACC-003, visual half) as a direct product-owner
 * decision — real, intended future scope, deliberately *deferred*, not an
 * unexplained gap and not descoped. Unlike the other three Case-C pages
 * touched in this same pass, this one's revisit condition is not an
 * independent trigger of its own — it is practically gated on Context
 * Ingestion (ACC-002, itself deferred, §2 of the same document): there is
 * nothing real to visualize until ingestion exists. "Not implemented" was
 * accurate but generic; the copy below now names that real dependency
 * instead of leaving a reader to guess. Still no service-topology,
 * dependency-graph, or call-flow-tracing entity exists in the backend
 * today — that underlying fact is unchanged, only how plainly it's
 * explained.
 */

import React from 'react';
import { Layers } from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';

export const FeatureMapFlowPage: React.FC = () => {
  return (
    <div id="feature-map-flow-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Feature Map / System Call Flow"
        subtitle="Service topology, dependency graph, and call-flow tracing"
        badge="DEFERRED — REAL FUTURE SCOPE"
        icon={Layers}
      />

      <EmptyState
        icon={Layers}
        badge="DEFERRED, NOT DESCOPED"
        title="Feature mapping is real, deferred scope — gated on Context Ingestion"
        description="A visual quality-graph/traceability UI remains real, intended product scope — deliberately deferred, not rejected, per a direct product-owner decision (AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md §2, 22 August 2026). Revisit condition: practically gated on Context Ingestion (ACC-002, itself deferred) — there is nothing real to visualize without it. No service-topology, dependency-graph, or call-flow-tracing entity exists in the backend today."
        codeHint="ACC-003 (visual half) — AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md §2"
      />
    </div>
  );
};
