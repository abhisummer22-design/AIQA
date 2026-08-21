/**
 * FE-11 — Mobile Testing Hub: truthful "not implemented" page (Case C).
 *
 * The previous version rendered a fully fabricated device farm: 6
 * hardcoded `MobileDeviceTarget` records (specific Samsung/Pixel/Apple
 * models with invented battery/thermal telemetry), a fake device
 * lock/reserve toggle, and 4 hardcoded `MobileTestExecution` records with
 * a "run" button that only appended a new fake record via `setTimeout`.
 *
 * Re-verified against the real backend route table at the start of this
 * sprint: there is no device-inventory entity, no device lock/reservation
 * concept, and no mobile-coverage-type dispatch endpoint anywhere in the
 * backend — a page named "Mobile Testing" is not evidence such a
 * capability exists (brief §9). Nothing about device farms, physical or
 * virtual, exists in this system.
 *
 * Per this sprint's Case C rule: replace the fabricated device farm with a
 * truthful "not implemented" state rather than inventing a fallback
 * device-inventory model.
 */

import React from 'react';
import { useNavigate } from 'react-router-dom';
import { Smartphone } from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { EmptyState } from '../common/EmptyState';

export const MobileTestingPage: React.FC = () => {
  const navigate = useNavigate();

  return (
    <div id="mobile-testing-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Mobile Testing Hub"
        subtitle="Device fleet selection and non-UI mobile coverage execution"
        badge="NOT IMPLEMENTED"
        icon={Smartphone}
      />

      <EmptyState
        icon={Smartphone}
        badge="NO BACKEND CAPABILITY"
        title="Mobile device testing is not implemented"
        description="The backend has no device-inventory, device-lock/reservation, or mobile-coverage-dispatch entity — this page previously showed a fabricated device farm instead of a real feature. Real test execution is scheduled and run through the Execution Hub."
        actionLabel="Go to Execution Hub"
        onAction={() => navigate('/execution')}
        codeHint="No corresponding route exists in services/orchestrator/src/api.rs"
      />
    </div>
  );
};
