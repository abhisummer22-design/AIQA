/**
 * MetricCard Component
 * Truthful, authoritative metrics display with high contrast and subtle borders
 */

import React from 'react';
import { LucideIcon } from 'lucide-react';

interface MetricCardProps {
  label: string;
  value: string | number;
  subtext?: string;
  icon?: LucideIcon;
  variant?: 'default' | 'success' | 'warning' | 'danger' | 'info';
  statusBadge?: string;
  className?: string;
}

export const MetricCard: React.FC<MetricCardProps> = ({
  label,
  value,
  subtext,
  icon: Icon,
  variant = 'default',
  statusBadge,
  className = '',
}) => {
  const getVariantStyles = () => {
    switch (variant) {
      case 'success':
        return {
          border: 'border-emerald-900/60 hover:border-emerald-800/80',
          iconBg: 'bg-emerald-950/80 text-emerald-400 border-emerald-800/40',
          valueColor: 'text-emerald-300',
        };
      case 'warning':
        return {
          border: 'border-amber-900/60 hover:border-amber-800/80',
          iconBg: 'bg-amber-950/80 text-amber-400 border-amber-800/40',
          valueColor: 'text-amber-300',
        };
      case 'danger':
        return {
          border: 'border-rose-900/60 hover:border-rose-800/80',
          iconBg: 'bg-rose-950/80 text-rose-400 border-rose-800/40',
          valueColor: 'text-rose-300',
        };
      case 'info':
        return {
          border: 'border-cyan-900/60 hover:border-cyan-800/80',
          iconBg: 'bg-cyan-950/80 text-cyan-400 border-cyan-800/40',
          valueColor: 'text-cyan-300',
        };
      default:
        return {
          border: 'border-slate-800/80 hover:border-slate-700/80',
          iconBg: 'bg-slate-800/80 text-slate-400 border-slate-700/50',
          valueColor: 'text-slate-100',
        };
    }
  };

  const styles = getVariantStyles();

  return (
    <div
      className={`relative p-3.5 rounded-lg border bg-[#0B1120] transition-all duration-150 ${styles.border} ${className}`}
    >
      <div className="flex items-start justify-between gap-2 mb-1.5">
        <span className="text-[10px] font-semibold text-slate-400 tracking-wider uppercase font-mono">
          {label}
        </span>
        {Icon && (
          <div className={`p-1 rounded border ${styles.iconBg}`}>
            <Icon className="w-3.5 h-3.5" />
          </div>
        )}
      </div>

      <div className="flex items-baseline justify-between gap-2">
        <div className={`text-xl font-bold font-mono tracking-tight ${styles.valueColor}`}>
          {value}
        </div>
        {statusBadge && (
          <span className="text-[10px] font-mono font-medium px-1.5 py-0.2 rounded bg-[#020617] border border-slate-800 text-slate-300">
            {statusBadge}
          </span>
        )}
      </div>

      {subtext && <p className="mt-0.5 text-[11px] text-slate-400 font-sans">{subtext}</p>}
    </div>
  );
};
