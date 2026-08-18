/**
 * Quality & Defect Reports View (Contract §21)
 * Generates authoritative compliance, test execution, and RCA defect audit reports with PDF/Excel export.
 */

import React from 'react';
import {
  FileText,
  Download,
  FileSpreadsheet,
  FileCheck,
  Calendar,
  Layers,
  CheckCircle2,
  ShieldAlert,
} from 'lucide-react';
import { SectionHeader } from '../../components/common/SectionHeader';
import { BackendNotice } from '../../components/common/BackendNotice';
import { Card } from '../../components/common/Card';
import { MetricCard } from '../../components/common/MetricCard';

export function ReportsView() {
  return (
    <div className="space-y-4">
      <SectionHeader
        title="Quality & Defect Reports"
        subtitle="Formal compliance, defect summary, and execution audit reports with exportable artifacts."
        badge="STAGED SPECIFICATION"
        badgeTone="warn"
      />

      <BackendNotice
        feature="Quality Report Generation & Exports"
        contractSection="§21 (Reports APIs)"
        endpoint="GET /api/v1/projects/{project_id}/reports | POST /api/v1/projects/{project_id}/reports"
        requires="Report aggregation engine and PDF/Excel exporter."
        payloadExample={`// POST /api/v1/projects/{project_id}/reports
{
  "report_type": "FINAL_AUDIT_REPORT",
  "scope": "PROJECT",
  "include_evidence": true,
  "format": "PDF"
}`}
      />

      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        <MetricCard label="Generated Audit Reports" value="—" subtext="BACKEND API REQUIRED" icon={<FileText size={14} />} />
        <MetricCard label="Exported Defect Summaries" value="—" subtext="BACKEND API REQUIRED" icon={<ShieldAlert size={14} />} />
        <MetricCard label="Compliance Score" value="—" subtext="BACKEND API REQUIRED" icon={<FileCheck size={14} />} />
        <MetricCard label="Historical Archives" value="—" subtext="BACKEND API REQUIRED" icon={<Calendar size={14} />} />
      </div>

      <Card icon={<FileText size={14} />} title="Report Archive & Exports">
        <div className="py-10 px-4 text-center space-y-2">
          <div className="text-xs text-slate-500 font-mono">
            No generated reports. Staging state ready.
          </div>
          <p className="text-[11px] text-slate-600 max-w-md mx-auto">
            Once report generation endpoints are added to the orchestrator, executive summaries, bug logs, and compliance PDFs will be generated and downloaded here.
          </p>
        </div>
      </Card>
    </div>
  );
}
