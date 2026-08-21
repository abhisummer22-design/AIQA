/**
 * FE-11 — API Testing & Protocol Adapters: truthful "not implemented" page
 * (Case C).
 *
 * The previous version rendered a fully client-side, fabricated API test
 * harness: hardcoded `APIEndpointTest` records, a fake multi-protocol
 * adapter selector (REST/GraphQL/gRPC/WebSocket) with no real dispatch
 * behind any of them, and "Execute Endpoint"/"Send" buttons that only
 * mutated local demo state with a canned response body.
 *
 * Re-verified against the real backend route table at the start of this
 * sprint: there is no API-test-definition entity and no endpoint that
 * dispatches an arbitrary HTTP/GraphQL/gRPC/WebSocket request on the
 * backend's behalf — a page named "API Testing" is not evidence such a
 * capability exists (brief §9). The backend's own real API surface is the
 * one already fully typed and exercised by this frontend's own real pages
 * (`aiqaApi.ts`) — there is nothing to "test" through a second, generic
 * request-builder UI that this app doesn't already call directly and
 * correctly.
 *
 * Per this sprint's Case C rule: replace the fabricated request builder
 * with a truthful "not implemented" state rather than inventing a
 * fallback multi-protocol dispatch layer.
 */

import React from 'react';
import { useNavigate } from 'react-router-dom';
import { Network } from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';

export const APITestingPage: React.FC = () => {
  const navigate = useNavigate();

  return (
    <div id="api-testing-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="API Testing & Protocol Adapters"
        subtitle="Multi-protocol request/response test harness"
        badge="NOT IMPLEMENTED"
        icon={Network}
      />

      <EmptyState
        icon={Network}
        badge="NO BACKEND CAPABILITY"
        title="API test harness is not implemented"
        description="The backend has no API-test-definition entity or generic HTTP/GraphQL/gRPC/WebSocket dispatch endpoint — this page previously showed a fabricated request builder instead of a real feature. Real API tasks are scheduled and their results reviewed through the Execution Hub."
        actionLabel="Go to Execution Hub"
        onAction={() => navigate('/execution')}
        codeHint="No corresponding route exists in services/orchestrator/src/api.rs"
      />
    </div>
  );
};
