/**
 * Mobile Testing Hub View (Contract §19)
 * Manages device pools, hardware capability checks, locking, and Maestro flow executions.
 */

import React from 'react';
import {
  Smartphone,
  Cpu,
  Lock,
  Unlock,
  Layers,
  Activity,
  Play,
  CheckCircle2,
} from 'lucide-react';
import { SectionHeader } from '../../components/common/SectionHeader';
import { BackendNotice } from '../../components/common/BackendNotice';
import { Card } from '../../components/common/Card';
import { MetricCard } from '../../components/common/MetricCard';

export function MobileTestingView() {
  return (
    <div className="space-y-4">
      <SectionHeader
        title="Mobile Device Testing Hub"
        subtitle="Automated Android/Samsung & iOS/Apple device pool management, hardware locks, and Maestro flow executions."
        badge="STAGED SPECIFICATION"
        badgeTone="warn"
      />

      <BackendNotice
        feature="Mobile Device Pool Management"
        contractSection="§19 (Mobile APIs)"
        endpoint="GET /api/v1/projects/{project_id}/mobile/devices | POST .../mobile/executions"
        requires="Device farm inventory and device mutex lock endpoints. (The backend includes a Maestro executor in executors/maestro, but exposes no device management API)."
        payloadExample={`// POST /api/v1/projects/{project_id}/mobile/devices/{device_id}/lock
{
  "lease_duration_sec": 300,
  "flow_id": "flow_checkout_01"
}`}
      />

      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricCard label="Registered Devices" value="—" subtext="BACKEND API REQUIRED" icon={<Smartphone size={14} />} />
        <MetricCard label="Active Hardware Locks" value="—" subtext="BACKEND API REQUIRED" icon={<Lock size={14} />} />
        <MetricCard label="Maestro Flows" value="—" subtext="BACKEND API REQUIRED" icon={<Play size={14} />} />
        <MetricCard label="Pass Rate Across OS" value="—" subtext="BACKEND API REQUIRED" icon={<CheckCircle2 size={14} />} />
      </div>

      <Card icon={<Smartphone size={14} />} title="Connected Device Farm">
        <div className="py-10 px-4 text-center space-y-2">
          <div className="text-xs text-slate-500 font-mono">
            No physical or emulated devices connected. Staging state ready.
          </div>
          <p className="text-[11px] text-slate-600 max-w-md mx-auto">
            When the mobile device farm API is exposed, attached devices (Android APK / iOS Simulator) and their locking status will be displayed here.
          </p>
        </div>
      </Card>
    </div>
  );
}
