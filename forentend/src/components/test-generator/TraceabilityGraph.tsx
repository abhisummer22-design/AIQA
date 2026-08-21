import React from 'react';
import {
  FileText,
  Bookmark,
  Sparkles,
  CheckCircle2,
  Tag,
  Play,
  FileCheck2,
  FileSpreadsheet,
  ArrowRight,
  GitBranch,
  Clock,
  CircleDashed,
  AlertCircle,
} from 'lucide-react';
import { TraceabilityChain } from '../../types';

interface TraceabilityGraphProps {
  traceability?: TraceabilityChain;
  testCaseId: string;
  category: string;
  sourceDoc?: string;
  requirementRef?: string;
  testCaseStatus?: 'PENDING_REVIEW' | 'APPROVED' | 'REJECTED' | 'EXPORTED_TO_EXECUTION';
  hasEvidence?: boolean;
}

export const TraceabilityGraph: React.FC<TraceabilityGraphProps> = ({
  traceability,
  testCaseId,
  category,
  sourceDoc = 'PRD / FRD / SRD',
  requirementRef = 'Requirement Spec',
  testCaseStatus = 'PENDING_REVIEW',
  hasEvidence = false,
}) => {
  // Determine realistic workflow state for execution stage
  const isDispatched = testCaseStatus === 'EXPORTED_TO_EXECUTION';
  const isApproved = testCaseStatus === 'APPROVED';

  const executionStateLabel = isDispatched
    ? 'Queued'
    : isApproved
    ? 'Ready'
    : 'Not Started';

  const executionStateColor = isDispatched
    ? 'text-indigo-400 border-indigo-700/60 bg-indigo-950/40'
    : isApproved
    ? 'text-cyan-400 border-cyan-700/60 bg-cyan-950/40'
    : 'text-slate-400 border-slate-700/60 bg-slate-900/60';

  const evidenceStateLabel = hasEvidence
    ? 'Captured'
    : isDispatched
    ? 'Awaiting Run'
    : 'Not Captured';

  const evidenceStateColor = hasEvidence
    ? 'text-emerald-400 border-emerald-700/60 bg-emerald-950/40'
    : 'text-slate-400 border-slate-700/60 bg-slate-900/60';

  const reportStateLabel = hasEvidence ? 'Ready for Audit' : 'Not Started';
  const reportStateColor = hasEvidence
    ? 'text-emerald-400 border-emerald-700/60 bg-emerald-950/40'
    : 'text-slate-400 border-slate-700/60 bg-slate-900/60';

  const steps = [
    {
      id: 'src-doc',
      stepNum: '01',
      label: 'Source Doc',
      value: traceability?.sourceDoc || sourceDoc,
      icon: FileText,
      badge: 'INPUT',
      status: 'AVAILABLE',
      stateCategory: 'COMPLETED',
      color: 'border-cyan-500/40 bg-cyan-950/30 text-cyan-200',
      iconColor: 'text-cyan-400',
    },
    {
      id: 'req',
      stepNum: '02',
      label: 'Requirement',
      value: traceability?.requirementRef || requirementRef,
      icon: Bookmark,
      badge: 'SPEC',
      status: 'LINKED',
      stateCategory: 'COMPLETED',
      color: 'border-indigo-500/40 bg-indigo-950/30 text-indigo-200',
      iconColor: 'text-indigo-400',
    },
    {
      id: 'ai',
      stepNum: '03',
      label: 'Generation Method',
      // FE-16: was `traceability?.aiAnalysis || 'Synthesized via Gemini
      // Quality Engine'` — both the field and this fallback made a false
      // AI-synthesis claim. `aiAnalysis` is now itself always a truthful
      // string (see `TestCaseGeneratorPage.tsx`); this fallback covers only
      // the case where no `traceability` exists at all.
      value: traceability?.aiAnalysis || 'Locally assembled from template (no AI model called)',
      icon: Sparkles,
      badge: 'LOCAL',
      status: 'TEMPLATE',
      stateCategory: 'COMPLETED',
      color: 'border-purple-500/40 bg-purple-950/30 text-purple-200',
      iconColor: 'text-purple-400',
    },
    {
      id: 'tc',
      stepNum: '04',
      label: 'Generated Case',
      value: traceability?.testCaseId || testCaseId,
      icon: CheckCircle2,
      badge: 'ACTIVE ID',
      status: 'GENERATED',
      stateCategory: 'COMPLETED',
      color: 'border-emerald-500/40 bg-emerald-950/30 text-emerald-200',
      iconColor: 'text-emerald-400',
    },
    {
      id: 'cat',
      stepNum: '05',
      label: 'Category',
      value: traceability?.testCategory || category,
      icon: Tag,
      badge: 'STRATEGY',
      status: 'ASSIGNED',
      stateCategory: 'COMPLETED',
      color: 'border-amber-500/40 bg-amber-950/30 text-amber-200',
      iconColor: 'text-amber-400',
    },
    {
      id: 'exec',
      stepNum: '06',
      label: 'Execution Pool',
      value: traceability?.executionTarget || 'k8s-runner-pool-us (Target)',
      icon: Play,
      badge: 'RUNNER',
      status: executionStateLabel,
      stateCategory: isDispatched ? 'IN_PROGRESS' : 'PENDING',
      color: executionStateColor,
      iconColor: isDispatched ? 'text-indigo-400' : 'text-slate-400',
    },
    {
      id: 'evid',
      stepNum: '07',
      label: 'Evidence Policy',
      value: traceability?.evidencePolicy || 'HAR + DOM Trace (Policy)',
      icon: FileCheck2,
      badge: 'ARTIFACT',
      status: evidenceStateLabel,
      stateCategory: hasEvidence ? 'COMPLETED' : 'PENDING',
      color: evidenceStateColor,
      iconColor: hasEvidence ? 'text-emerald-400' : 'text-slate-400',
    },
    {
      id: 'rep',
      stepNum: '08',
      label: 'Compliance Report',
      value: traceability?.reportMapping || 'SOC2 Matrix (Policy)',
      icon: FileSpreadsheet,
      badge: 'SIGNOFF',
      status: reportStateLabel,
      stateCategory: hasEvidence ? 'COMPLETED' : 'PENDING',
      color: reportStateColor,
      iconColor: hasEvidence ? 'text-emerald-400' : 'text-slate-400',
    },
  ];

  return (
    <div
      id="traceability-chain-viewer"
      className="p-4 rounded-lg bg-[#040817] border border-slate-800 space-y-3.5 shadow-inner"
    >
      <div className="flex items-center justify-between flex-wrap gap-2 pb-2 border-b border-slate-800/80">
        <div className="flex items-center gap-2">
          <div className="p-1 rounded bg-cyan-950/60 border border-cyan-700/60 text-cyan-400">
            <GitBranch className="w-4 h-4" />
          </div>
          <div>
            <h4 className="text-xs font-bold font-sans text-slate-100 tracking-wide">
              Requirement Traceability & Lifecycle Chain
            </h4>
            <p className="text-[11px] text-slate-400 font-sans">
              State-aware lifecycle from input specification to future execution artifacts
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <span className="text-[10px] font-mono font-bold text-slate-400 bg-slate-900 px-2 py-0.5 rounded border border-slate-800">
            Steps 01–05: <span className="text-emerald-400">Prepared</span>
          </span>
          <span className="text-[10px] font-mono font-bold text-slate-400 bg-slate-900 px-2 py-0.5 rounded border border-slate-800">
            Steps 06–08: <span className="text-amber-400">Downstream Workflow</span>
          </span>
        </div>
      </div>

      {/* Responsive Step Chain */}
      <div className="grid grid-cols-2 sm:grid-cols-4 xl:grid-cols-8 gap-2">
        {steps.map((step, idx) => {
          const Icon = step.icon;
          return (
            <div
              key={step.id}
              className={`relative flex flex-col justify-between p-2.5 rounded-lg border transition-all ${
                step.stateCategory === 'COMPLETED'
                  ? 'bg-[#0B1120] border-slate-700/80'
                  : 'bg-[#060a14] border-slate-800/70 opacity-90'
              }`}
            >
              <div className="flex items-center justify-between mb-1.5">
                <span className="text-[10px] font-mono font-bold text-slate-400">
                  #{step.stepNum}
                </span>
                <span className={`text-[9px] font-mono font-bold px-1.5 py-0.2 rounded border ${step.color}`}>
                  {step.status}
                </span>
              </div>

              <div className="flex items-center gap-1.5 mb-1 text-slate-300">
                <Icon className={`w-3.5 h-3.5 ${step.iconColor} shrink-0`} />
                <span className="text-[11px] font-semibold font-sans truncate">
                  {step.label}
                </span>
              </div>

              <div
                className="text-[11px] font-mono font-medium text-slate-100 truncate bg-[#020617] px-1.5 py-1 rounded border border-slate-800/80"
                title={step.value}
              >
                {step.value}
              </div>

              {idx < steps.length - 1 && (
                <div className="hidden xl:flex absolute -right-2 top-1/2 -translate-y-1/2 z-10 w-4 h-4 rounded-full bg-[#0B1120] border border-slate-700 items-center justify-center pointer-events-none text-slate-500">
                  <ArrowRight className="w-2.5 h-2.5" />
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
};
