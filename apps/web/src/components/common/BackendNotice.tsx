/**
 * BackendNotice Component
 * Prominently documents the required backend API dependency for staged views
 * (Audit Brief Rule: "Use BACKEND API REQUIRED when a feature cannot yet be connected").
 */

import React from 'react';
import { AlertCircle, ArrowRight, Code2, Database, Layers } from 'lucide-react';

interface BackendNoticeProps {
  feature: string;
  contractSection: string;
  endpoint: string;
  requires: string;
  payloadExample?: string;
  className?: string;
}

export const BackendNotice: React.FC<BackendNoticeProps> = ({
  feature,
  contractSection,
  endpoint,
  requires,
  payloadExample,
  className = '',
}) => {
  return (
    <div
      className={`rounded-xl border border-amber-800/60 bg-amber-950/20 p-4 text-xs text-slate-300 space-y-2.5 ${className}`}
    >
      <div className="flex flex-wrap items-center justify-between gap-2 border-b border-amber-900/40 pb-2">
        <div className="flex items-center gap-2">
          <span className="flex h-2 w-2 rounded-full bg-amber-400 animate-pulse" />
          <span className="font-mono text-[10px] font-bold uppercase tracking-widest text-amber-400">
            BACKEND API REQUIRED
          </span>
        </div>
        <span className="font-mono text-[10px] text-slate-400">
          Contract {contractSection}
        </span>
      </div>

      <div className="space-y-1">
        <div className="font-semibold text-slate-100 flex items-center gap-2">
          <Layers size={14} className="text-amber-400" />
          <span>{feature}</span>
        </div>
        <p className="text-[11px] leading-relaxed text-slate-400">{requires}</p>
      </div>

      <div className="flex flex-col sm:flex-row sm:items-center gap-2 pt-1 font-mono text-[11px]">
        <span className="text-slate-500 shrink-0">Required Endpoint:</span>
        <code className="rounded bg-slate-900/80 border border-slate-800 px-2 py-0.5 text-cyan-300 break-all">
          {endpoint}
        </code>
      </div>

      {payloadExample && (
        <details className="mt-2 text-[10px] font-mono group">
          <summary className="cursor-pointer text-slate-500 hover:text-slate-400 flex items-center gap-1">
            <Code2 size={12} />
            <span>View expected contract payload</span>
          </summary>
          <pre className="mt-1.5 p-2.5 rounded bg-slate-950/90 border border-slate-800 text-slate-300 overflow-x-auto text-[10px] leading-relaxed">
            {payloadExample}
          </pre>
        </details>
      )}
    </div>
  );
};
