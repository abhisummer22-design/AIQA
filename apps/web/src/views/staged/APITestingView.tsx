/**
 * API Testing Console View (Contract §18)
 * Manages endpoint specifications, contract fuzzing, and API telemetry metrics.
 */

import React from 'react';
import {
  Network,
  Activity,
  Code2,
  Sliders,
  Play,
  CheckCircle2,
  Clock,
  Zap,
} from 'lucide-react';
import { SectionHeader } from '../../components/common/SectionHeader';
import { BackendNotice } from '../../components/common/BackendNotice';
import { Card } from '../../components/common/Card';
import { MetricCard } from '../../components/common/MetricCard';

export function APITestingView() {
  return (
    <div className="space-y-4">
      <SectionHeader
        title="API Testing & Telemetry Console"
        subtitle="OpenAPI/GraphQL contract verification, Schemathesis fuzzing, Newman collection runs, and latency telemetry."
        badge="STAGED SPECIFICATION"
        badgeTone="warn"
      />

      <BackendNotice
        feature="API Specification & Telemetry Engine"
        contractSection="§18 (API Testing APIs)"
        endpoint="GET /api/v1/projects/{project_id}/api/specifications | GET /api/v1/projects/{project_id}/api/telemetry"
        requires="API spec catalogue and real-time telemetry collector. (Schemathesis and Newman executors exist on the backend but are only invoked via RunPlan)."
        payloadExample={`// POST /api/v1/projects/{project_id}/api/executions
{
  "specification_id": "spec_auth_v2",
  "endpoint": "/api/v2/oauth/token",
  "method": "POST",
  "fuzz_parameters": true,
  "expected_status": 200
}`}
      />

      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricCard label="Audited Endpoints" value="—" subtext="BACKEND API REQUIRED" icon={<Network size={14} />} />
        <MetricCard label="Contract Pass Ratio" value="—" subtext="BACKEND API REQUIRED" icon={<CheckCircle2 size={14} />} />
        <MetricCard label="p95 Latency" value="—" subtext="BACKEND API REQUIRED" icon={<Clock size={14} />} />
        <MetricCard label="Schema Drift Alerts" value="—" subtext="BACKEND API REQUIRED" icon={<Zap size={14} />} />
      </div>

      <Card icon={<Network size={14} />} title="API Endpoint Directory & Test Harness">
        <div className="py-10 px-4 text-center space-y-2">
          <div className="text-xs text-slate-500 font-mono">
            No API specifications connected. Staging state ready.
          </div>
          <p className="text-[11px] text-slate-600 max-w-md mx-auto">
            Once the API specification endpoints are available, individual REST/GraphQL endpoints can be inspected, fuzzed with Schemathesis, and tested directly.
          </p>
        </div>
      </Card>
    </div>
  );
}
