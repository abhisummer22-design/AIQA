/**
 * EmptyState Component
 * Truthful, actionable empty & unconfigured states for AIQA modules
 */

import React from 'react';
import { LucideIcon, Inbox, ArrowRight } from 'lucide-react';

interface EmptyStateProps {
  icon?: LucideIcon;
  title: string;
  description: string;
  actionLabel?: string;
  onAction?: () => void;
  secondaryActionLabel?: string;
  onSecondaryAction?: () => void;
  badge?: string;
  codeHint?: string;
  className?: string;
}

export const EmptyState: React.FC<EmptyStateProps> = ({
  icon: Icon = Inbox,
  title,
  description,
  actionLabel,
  onAction,
  secondaryActionLabel,
  onSecondaryAction,
  badge,
  codeHint,
  className = '',
}) => {
  return (
    <div
      className={`flex flex-col items-center justify-center text-center p-6 md:p-8 rounded-lg border border-slate-800 bg-[#0B1120] ${className}`}
    >
      <div className="w-10 h-10 rounded-lg bg-[#020617] border border-slate-700/60 flex items-center justify-center mb-3 text-violet-400">
        <Icon className="w-5 h-5" />
      </div>

      {badge && (
        <span className="mb-1.5 inline-flex items-center px-2 py-0.2 rounded text-[10px] font-mono font-medium bg-[#020617] text-slate-300 border border-slate-800">
          {badge}
        </span>
      )}

      <h3 className="text-sm font-semibold text-slate-200 mb-1">{title}</h3>
      <p className="text-xs text-slate-400 max-w-md mb-4 leading-relaxed">{description}</p>

      {codeHint && (
        <div className="mb-4 px-3 py-1.5 rounded bg-[#020617] border border-slate-800 font-mono text-[11px] text-slate-400 select-all max-w-lg overflow-x-auto text-left">
          <code>{codeHint}</code>
        </div>
      )}

      {(actionLabel || secondaryActionLabel) && (
        <div className="flex flex-wrap items-center justify-center gap-2.5">
          {actionLabel && (
            <button
              onClick={onAction}
              className="inline-flex items-center gap-1.5 px-3.5 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 text-white font-semibold text-xs transition-all shadow-sm shadow-violet-950/60 cursor-pointer"
            >
              <span>{actionLabel}</span>
              <ArrowRight className="w-3.5 h-3.5" />
            </button>
          )}

          {secondaryActionLabel && (
            <button
              onClick={onSecondaryAction}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 font-medium text-xs border border-slate-700 transition-all cursor-pointer"
            >
              <span>{secondaryActionLabel}</span>
            </button>
          )}
        </div>
      )}
    </div>
  );
};
