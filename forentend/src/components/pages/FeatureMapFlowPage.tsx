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
 * Per this sprint's Case C rule: replace the fabricated topology/trace
 * viewer with a truthful "not implemented" state rather than inventing a
 * fallback architecture diagram.
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
        badge="NOT IMPLEMENTED"
        icon={Layers}
      />

      <EmptyState
        icon={Layers}
        badge="NO BACKEND CAPABILITY"
        title="Feature mapping is not implemented"
        description="The backend has no service-topology, dependency-graph, or call-flow-tracing entity — this page previously showed a fabricated microservice mesh with invented services and traces, unrelated to this application's actual architecture (a single orchestrator service plus this frontend)."
        codeHint="No corresponding route exists in services/orchestrator/src/api.rs"
      />
    </div>
  );
};
