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
 * Per that sprint's Case C rule, the fabricated device farm was replaced
 * with a truthful "not implemented" state rather than a fallback
 * device-inventory model.
 *
 * **Copy updated 22 August 2026** (a second, later, copy-only pass — no
 * backend/frontend behavior changed): `AIQA_PRD_FRD_SRD_v6.1_SCOPE_
 * CORRECTIONS.md` §2 has since recorded real device/emulator dispatch
 * (ACC-014) as a direct product-owner decision — real, intended future
 * scope, deliberately *deferred* pending a real budget decision, not an
 * unexplained gap and not descoped. "Not implemented" was accurate but
 * generic; the copy below now names the actual, recorded revisit
 * condition instead of leaving a reader to guess. Still no
 * device-inventory, device-lock/reservation, or mobile-coverage-dispatch
 * entity exists in the backend today — that underlying fact is unchanged,
 * only how plainly it's explained.
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
        badge="DEFERRED — REAL FUTURE SCOPE"
        icon={Smartphone}
      />

      <EmptyState
        icon={Smartphone}
        badge="DEFERRED, NOT DESCOPED"
        title="Mobile device testing is real, deferred scope — pending a device-farm budget decision"
        description="Real device/emulator dispatch remains real, intended product scope — deliberately deferred, not rejected, per a direct product-owner decision (AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md §2, 22 August 2026). Revisit condition: a real device-farm budget decision, separate from that document — matching the PRD's own existing 'post-MVP' precedent for adjacent mobile tooling (Appium/Strix). No device-inventory, device-lock/reservation, or mobile-coverage-dispatch entity exists in the backend today."
        actionLabel="Go to Execution Hub"
        onAction={() => navigate('/execution')}
        codeHint="ACC-014 — AIQA_PRD_FRD_SRD_v6.1_SCOPE_CORRECTIONS.md §2"
      />
    </div>
  );
};
