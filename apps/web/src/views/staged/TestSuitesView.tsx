/**
 * Test Suites & Execution Plans View (Contract §10)
 * Manages reusable suites and execution plan topologies.
 */

import React from 'react';
import {
  ListChecks,
  Plus,
  Layers,
  Workflow,
  Search,
  CheckCircle2,
  Clock,
} from 'lucide-react';
import { SectionHeader } from '../../components/common/SectionHeader';
import { BackendNotice } from '../../components/common/BackendNotice';
import { Card } from '../../components/common/Card';
import { MetricCard } from '../../components/common/MetricCard';

export function TestSuitesView() {
  return (
    <div className="space-y-4">
      <SectionHeader
        title="Test Suites & Execution Plans"
        subtitle="Catalogue of persisted test suites, reusable regression matrices, and DAG execution plans."
        badge="STAGED SPECIFICATION"
        badgeTone="warn"
      />

      <BackendNotice
        feature="Test Suite & Plan Catalogue"
        contractSection="§10 (Test Suite / Plan APIs)"
        endpoint="GET /api/v1/projects/{project_id}/test-suites | POST /api/v1/projects/{project_id}/test-plans"
        requires="Persisted test suite groupings and execution plan definitions. (Currently, run plans are submitted whole to POST /runs without a persisted catalogue)."
        payloadExample={`// POST /api/v1/projects/{project_id}/test-plans
{
  "name": "Nightly Regression Matrix",
  "suite_ids": ["suite_auth_01", "suite_payments_02"],
  "environment": "staging",
  "execution_policy": "CONTINUE_ON_FAILURE",
  "parallelism": 8
}`}
      />

      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricCard label="Configured Suites" value="—" subtext="BACKEND API REQUIRED" icon={<ListChecks size={14} />} />
        <MetricCard label="Total Linked Tests" value="—" subtext="BACKEND API REQUIRED" icon={<Layers size={14} />} />
        <MetricCard label="Active Execution Plans" value="—" subtext="BACKEND API REQUIRED" icon={<Workflow size={14} />} />
        <MetricCard label="Average Execution Time" value="—" subtext="BACKEND API REQUIRED" icon={<Clock size={14} />} />
      </div>

      <Card icon={<ListChecks size={14} />} title="Suite Directory">
        <div className="py-10 px-4 text-center space-y-2">
          <div className="text-xs text-slate-500 font-mono">
            No persisted test suites. Staging state ready.
          </div>
          <p className="text-[11px] text-slate-600 max-w-md mx-auto">
            Once test suite endpoints are exposed by the orchestrator, test collections and custom execution plans will be managed and scheduled from this screen.
          </p>
        </div>
      </Card>
    </div>
  );
}
