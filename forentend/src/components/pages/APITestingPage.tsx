/**
 * API Testing & Protocol Adapters — honest redirect to the real capability.
 *
 * FE-11 (Case C) replaced this page's original, fully fabricated client-side
 * request harness (hardcoded `APIEndpointTest` records, a fake REST/GraphQL/
 * gRPC/WebSocket adapter selector with no real dispatch behind any of it)
 * with a "not implemented" state. That was correct about *this page's own*
 * capability — there is still no dedicated `APITestDefinition` entity or
 * generic HTTP/GraphQL/gRPC/WebSocket dispatch endpoint on the backend — but
 * phrasing it as "API testing is not implemented" overclaimed: real API
 * testing already shipped, just on a different page.
 *
 * `AIQA_PRODUCT_DECISIONS_RECORD.md` decision #3 (ACC-012, 22 Aug 2026)
 * closes this, in the product owner's own words: "The underlying capability
 * is real — Newman/Pytest/Schemathesis run via the Execution Hub, this is
 * real API testing... update that page so it clearly says 'this capability
 * is in the Execution Hub, go there' — remove the 'not implemented'
 * message." Verified for real against `services/orchestrator/src/
 * executor.rs` and the repo root `README.md`: `PytestExecutor`,
 * `NewmanExecutor`, and `SchemathesisExecutor` each shell out to their real
 * CLI tool and are verified end-to-end against the orchestrator's own
 * running server — a task's `executor` field (set on the Execution Hub's own
 * task form, not here) is how you choose one.
 *
 * What this does not change (same decision, explicitly deferred): whether a
 * dedicated, rich API-testing authoring UI (collections, environments,
 * request history) is ever built. This page stays a truthful pointer to the
 * Execution Hub, not a new feature of its own.
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
        subtitle="Real API testing runs through the Execution Hub, not a separate screen here"
        badge="VIA EXECUTION HUB"
        icon={Network}
      />

      <EmptyState
        icon={Network}
        badge="IMPLEMENTED ELSEWHERE"
        title="API testing runs through the Execution Hub"
        description="This page has no dedicated request-builder UI, and the previous version's one was fabricated demo data rather than a real feature. Real API tests run as Execution Hub tasks — create or open a run there and set a task's executor to pytest, Newman, or Schemathesis."
        actionLabel="Go to Execution Hub"
        onAction={() => navigate('/execution')}
        codeHint="TaskDef.executor = 'pytest' | 'newman' | 'schemathesis'"
      />
    </div>
  );
};
