/**
 * AIQA Reports & Quality Intelligence Workspace
 * Authoritative report summaries, failure taxonomy, flakiness index, and defect drafts.
 */

import React, { useState } from 'react';
import {
  FileText,
  Download,
  Share2,
  CheckCircle2,
  XCircle,
  ServerCrash,
  ShieldAlert,
  HelpCircle,
  Layers,
  Printer,
  ChevronRight,
  Sparkles,
  ExternalLink,
  Code2
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { SectionHeader } from '../common/SectionHeader';
import { StatusBadge, FailureClassificationBadge } from '../common/StatusBadge';
import { EmptyState } from '../common/EmptyState';
import { DefectDraft } from '../../types';

export const ReportsPage: React.FC = () => {
  const {
    runs,
    currentRun,
    setActiveRunId,
    qualityIntelligence,
    currentEnvironment,
  } = useAIQA();

  const [selectedRunId, setSelectedRunId] = useState<string>(currentRun?.id || runs[0]?.id || '');
  const [copySuccess, setCopySuccess] = useState(false);

  const reportRun = runs.find((r) => r.id === selectedRunId) || currentRun || runs[0];

  const initialDefectDrafts: DefectDraft[] = [
    {
      id: 'DEF-881',
      title: 'Auth Token Refresh accepts expired payload on revoked session',
      severity: 'CRITICAL',
      classification: 'PRODUCT_FAILURE',
      relatedTestId: 'test-102',
      reproductionSteps: [
        'Issue JWT bearer token for user_id 9912',
        'Invoke DELETE /api/v1/auth/sessions to revoke session',
        'POST /api/v1/auth/refresh with expired refresh cookie',
        'Observe HTTP 200 returned instead of expected HTTP 401 Unauthorized',
      ],
      suggestedFixContext: 'Token blacklist Redis lookup bypassed in refresh_token controller middleware.',
      jiraExportStatus: 'DRAFT',
    },
    {
      id: 'DEF-882',
      title: 'Auditor role permitted to execute DELETE on cold storage logs',
      severity: 'MAJOR',
      classification: 'PRODUCT_FAILURE',
      relatedTestId: 'rule-4',
      reproductionSteps: [
        'Authenticate as user with role "Auditor"',
        'Dispatch DELETE request to /v2/audit/storage/archive-2026-08',
        'Audit trail S3 delete marker was created',
      ],
      suggestedFixContext: 'IAM bucket policy missing explicit Deny on S3:DeleteObject for Auditor ARN.',
      jiraExportStatus: 'EXPORTED',
    },
  ];

  const [defectDrafts, setDefectDrafts] = useState<DefectDraft[]>(initialDefectDrafts);

  const handleExportJSON = () => {
    if (!reportRun) return;
    const dataStr = 'data:text/json;charset=utf-8,' + encodeURIComponent(JSON.stringify({
      runSummary: reportRun,
      qualityIntelligence,
      defectDrafts,
      generatedAt: new Date().toISOString(),
    }, null, 2));
    const downloadAnchor = document.createElement('a');
    downloadAnchor.setAttribute('href', dataStr);
    downloadAnchor.setAttribute('download', `aiqa_report_${reportRun.id}.json`);
    document.body.appendChild(downloadAnchor);
    downloadAnchor.click();
    downloadAnchor.remove();
  };

  const handleExportMarkdown = () => {
    if (!reportRun) return;
    const md = `# AIQA Execution & Quality Intelligence Report
Run ID: ${reportRun.id}
Environment: ${reportRun.environment}
Executor: ${reportRun.executor}
Status: ${reportRun.status}
Started: ${reportRun.startedAt}

## Results Distribution
- Total Tests: ${reportRun.totalTests}
- Passed: ${reportRun.counts.passed}
- Product Failures: ${reportRun.productFailuresCount}
- Infrastructure Failures: ${reportRun.infraFailuresCount}
- Needs Review: ${reportRun.counts.needsReview}

## Defect Triage
${defectDrafts.map((d) => `### ${d.id}: ${d.title} (${d.severity})
- Classification: ${d.classification}
- Suggested Context: ${d.suggestedFixContext}
`).join('\n')}
`;

    navigator.clipboard.writeText(md);
    setCopySuccess(true);
    setTimeout(() => setCopySuccess(false), 2000);
  };

  return (
    <div className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Quality & Defect Intelligence Reports"
        subtitle="Authoritative test results, failure taxonomy, trace coverage, and automated defect generation"
        badge={reportRun ? `REPORT: ${reportRun.id}` : 'REPORTS'}
        icon={FileText}
        actions={
          <div className="flex items-center gap-2">
            <button
              onClick={handleExportMarkdown}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 text-xs font-medium border border-slate-800 cursor-pointer"
            >
              <Share2 className="w-3.5 h-3.5" />
              <span>{copySuccess ? 'Copied Markdown!' : 'Copy Summary (MD)'}</span>
            </button>
            <button
              onClick={handleExportJSON}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer"
            >
              <Download className="w-3.5 h-3.5" />
              <span>Export JSON Report</span>
            </button>
          </div>
        }
      />

      {/* Select Run */}
      <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-3 flex flex-col sm:flex-row sm:items-center justify-between gap-3 text-xs">
        <div className="flex items-center gap-2">
          <span className="text-slate-400 font-medium">Select Authoritative Execution:</span>
          <select
            value={selectedRunId}
            onChange={(e) => {
              setSelectedRunId(e.target.value);
              setActiveRunId(e.target.value);
            }}
            className="px-2.5 py-1 rounded bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono text-xs"
          >
            {runs.map((r) => (
              <option key={r.id} value={r.id}>
                {r.id} — {r.title} ({r.status})
              </option>
            ))}
          </select>
        </div>
        <span className="text-slate-500 font-mono text-[10px]">
          Environment: <strong className="text-slate-300">{reportRun?.environment}</strong>
        </span>
      </div>

      {reportRun ? (
        <div className="space-y-4">
          {/* Executive Summary Card */}
          <div className="p-4 rounded-lg border border-slate-800 bg-[#0B1120] space-y-3.5">
            <div className="flex flex-col lg:flex-row lg:items-center justify-between gap-3 pb-3 border-b border-slate-800">
              <div className="space-y-0.5">
                <div className="flex items-center gap-2 flex-wrap">
                  <span className="font-mono text-xs font-bold text-indigo-400">{reportRun.id}</span>
                  <h2 className="text-sm font-bold text-slate-100">{reportRun.title}</h2>
                  <StatusBadge status={reportRun.status} size="sm" />
                </div>
                <p className="text-xs text-slate-400">
                  Executed via {reportRun.executor} with {reportRun.concurrency} parallel workers. Started {new Date(reportRun.startedAt).toLocaleString()}.
                </p>
              </div>

              {/* Pass Rate Metric */}
              <div className="flex items-center gap-3 bg-[#020617] px-3.5 py-2 rounded-lg border border-slate-800">
                <div>
                  <div className="text-[9px] font-mono text-slate-400 uppercase">Pass Rate</div>
                  <div className="text-lg font-bold font-mono text-emerald-400">
                    {Math.round((reportRun.counts.passed / (reportRun.totalTests || 1)) * 100)}%
                  </div>
                </div>
                <div className="h-6 w-[1px] bg-slate-800" />
                <div>
                  <div className="text-[9px] font-mono text-slate-400 uppercase">Evidence Coverage</div>
                  <div className="text-lg font-bold font-mono text-indigo-400">
                    100%
                  </div>
                </div>
              </div>
            </div>

            {/* Failure Taxonomy Breakout */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-2.5 text-xs">
              <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-1">
                <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
                  <span>Product Defect Ratio</span>
                  <ShieldAlert className="w-3.5 h-3.5 text-rose-400" />
                </div>
                <div className="text-lg font-bold font-mono text-rose-300">
                  {reportRun.productFailuresCount} Tests
                </div>
                <p className="text-[10px] text-slate-500">Violates business assertions or security policies</p>
              </div>

              <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-1">
                <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
                  <span>Infra / Network Errors</span>
                  <ServerCrash className="w-3.5 h-3.5 text-orange-400" />
                </div>
                <div className="text-lg font-bold font-mono text-orange-300">
                  {reportRun.infraFailuresCount} Tests
                </div>
                <p className="text-[10px] text-slate-500">Runner DNS or cluster timeouts (auto-isolated)</p>
              </div>

              <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-1">
                <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
                  <span>Flakiness Score</span>
                  <HelpCircle className="w-3.5 h-3.5 text-yellow-400" />
                </div>
                <div className="text-lg font-bold font-mono text-yellow-300">
                  {qualityIntelligence?.flakinessIndex || 0}%
                </div>
                <p className="text-[10px] text-slate-500">Historical run variance across 30 days</p>
              </div>
            </div>
          </div>

          {/* Automated Defect / Bug Drafts Section */}
          <div className="p-4 rounded-lg border border-slate-800 bg-[#0B1120] space-y-3">
            <div className="flex items-center justify-between pb-2.5 border-b border-slate-800">
              <div className="flex items-center gap-2">
                <Sparkles className="w-3.5 h-3.5 text-indigo-400" />
                <h3 className="text-xs font-bold text-slate-100 font-sans uppercase">
                  AI-Generated Defect Drafts & Jira Exports ({defectDrafts.length})
                </h3>
              </div>
              <span className="text-[9px] font-mono text-slate-400 bg-[#020617] px-2 py-0.5 rounded border border-slate-800">
                AUTONOMOUS TRIAGE
              </span>
            </div>

            <div className="space-y-2.5">
              {defectDrafts.map((defect) => (
                <div
                  key={defect.id}
                  className="p-3 rounded-lg border border-slate-800 bg-[#020617] space-y-2 text-xs"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="space-y-0.5">
                      <div className="flex items-center gap-1.5 flex-wrap">
                        <span className="font-mono font-bold text-indigo-400 text-xs">{defect.id}</span>
                        <span className="px-1.5 py-0.2 rounded bg-rose-950 border border-rose-800 text-rose-300 font-mono text-[9px] font-bold">
                          {defect.severity}
                        </span>
                        <FailureClassificationBadge classification={defect.classification} />
                        <span className="text-[10px] font-mono text-slate-500">
                          Source Test: <strong className="text-slate-300">{defect.relatedTestId}</strong>
                        </span>
                      </div>
                      <h4 className="text-xs font-bold text-slate-100 font-sans">{defect.title}</h4>
                    </div>

                    <span
                      className={`px-2 py-0.5 rounded font-mono text-[10px] font-semibold ${
                        defect.jiraExportStatus === 'EXPORTED'
                          ? 'bg-emerald-950 text-emerald-300 border border-emerald-800'
                          : 'bg-slate-800 text-slate-300 border border-slate-700'
                      }`}
                    >
                      {defect.jiraExportStatus === 'EXPORTED' ? 'SYNCED TO JIRA' : 'DRAFT'}
                    </span>
                  </div>

                  <div className="p-2.5 rounded bg-[#0B1120] border border-slate-800 space-y-1">
                    <div className="text-[9px] font-mono uppercase text-slate-400 font-bold">
                      Reproduction Path:
                    </div>
                    <ol className="list-decimal list-inside space-y-0.5 text-slate-300 font-mono text-[10px]">
                      {defect.reproductionSteps.map((s, idx) => (
                        <li key={idx}>{s}</li>
                      ))}
                    </ol>
                  </div>

                  <div className="flex items-center justify-between text-[10px] font-mono pt-0.5">
                    <div className="text-slate-400">
                      Root Cause Context: <span className="text-slate-200">{defect.suggestedFixContext}</span>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      ) : (
        <EmptyState
          icon={FileText}
          title="No Report Data Available"
          description="Reports are generated dynamically upon completing execution workloads."
        />
      )}
    </div>
  );
};
