/**
 * SectionHeader Component
 * Standardized section & page title bar with action slots and optional status pills.
 */

import React from 'react';

interface SectionHeaderProps {
  title: string;
  subtitle?: string;
  badge?: string;
  badgeTone?: 'info' | 'warn' | 'success' | 'live';
  action?: React.ReactNode;
  className?: string;
}

export const SectionHeader: React.FC<SectionHeaderProps> = ({
  title,
  subtitle,
  badge,
  badgeTone = 'info',
  action,
  className = '',
}) => {
  const getBadgeStyle = () => {
    switch (badgeTone) {
      case 'live':
        return 'bg-emerald-950/80 text-emerald-300 border-emerald-700/80 animate-pulse';
      case 'warn':
        return 'bg-amber-950/80 text-amber-300 border-amber-700/80';
      case 'success':
        return 'bg-emerald-950/80 text-emerald-300 border-emerald-700/80';
      default:
        return 'bg-indigo-950/80 text-indigo-300 border-indigo-700/80';
    }
  };

  return (
    <div className={`flex flex-col sm:flex-row sm:items-center justify-between gap-3 pb-3 border-b border-slate-800/80 ${className}`}>
      <div className="space-y-0.5">
        <div className="flex items-center gap-2 flex-wrap">
          <h1 className="text-lg font-bold tracking-tight text-slate-100 font-sans">{title}</h1>
          {badge && (
            <span
              className={`text-[10px] font-mono font-semibold uppercase px-2 py-0.5 rounded border tracking-wider ${getBadgeStyle()}`}
            >
              {badge}
            </span>
          )}
        </div>
        {subtitle && <p className="text-xs text-slate-400 font-sans leading-relaxed">{subtitle}</p>}
      </div>
      {action && <div className="flex items-center gap-2 shrink-0">{action}</div>}
    </div>
  );
};
