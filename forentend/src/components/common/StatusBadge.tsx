/**
 * StatusBadge Component
 * Implements the full AIQA status model + failure classification distinction
 */

import React from 'react';
import {
  CheckCircle2,
  XCircle,
  AlertOctagon,
  Clock,
  Loader2,
  Ban,
  Slash,
  AlertTriangle,
  ServerCrash,
  HelpCircle,
  ShieldAlert
} from 'lucide-react';
import { AIQAStatus, FailureClassification } from '../../types';

interface StatusBadgeProps {
  status: AIQAStatus;
  classification?: FailureClassification;
  size?: 'sm' | 'md' | 'lg';
  showIcon?: boolean;
  className?: string;
}

export const StatusBadge: React.FC<StatusBadgeProps> = ({
  status,
  classification,
  size = 'md',
  showIcon = true,
  className = '',
}) => {
  const getStatusConfig = () => {
    switch (status) {
      case 'PASSED':
        return {
          label: 'PASSED',
          bg: 'bg-emerald-950/60 border-emerald-800/80 text-emerald-300',
          icon: CheckCircle2,
          iconColor: 'text-emerald-400',
        };
      case 'FAILED':
        if (classification === 'INFRASTRUCTURE_FAILURE') {
          return {
            label: 'INFRA FAIL',
            bg: 'bg-orange-950/60 border-orange-700/80 text-orange-300',
            icon: ServerCrash,
            iconColor: 'text-orange-400',
          };
        }
        return {
          label: 'FAILED',
          bg: 'bg-rose-950/60 border-rose-800/80 text-rose-300',
          icon: XCircle,
          iconColor: 'text-rose-400',
        };
      case 'INFRA_ERROR':
        return {
          label: 'INFRA ERROR',
          bg: 'bg-amber-950/60 border-amber-700/80 text-amber-300',
          icon: ServerCrash,
          iconColor: 'text-amber-400',
        };
      case 'RUNNING':
        return {
          label: 'RUNNING',
          bg: 'bg-cyan-950/60 border-cyan-800/80 text-cyan-300',
          icon: Loader2,
          iconColor: 'text-cyan-400 animate-spin',
        };
      case 'QUEUED':
        return {
          label: 'QUEUED',
          bg: 'bg-slate-900 border-slate-700 text-slate-300',
          icon: Clock,
          iconColor: 'text-slate-400',
        };
      case 'BLOCKED':
        return {
          label: 'BLOCKED',
          bg: 'bg-purple-950/60 border-purple-800/80 text-purple-300',
          icon: Ban,
          iconColor: 'text-purple-400',
        };
      case 'SKIPPED':
        return {
          label: 'SKIPPED',
          bg: 'bg-zinc-900 border-zinc-700 text-zinc-400',
          icon: Slash,
          iconColor: 'text-zinc-500',
        };
      case 'CANCELLED':
        return {
          label: 'CANCELLED',
          bg: 'bg-zinc-900 border-zinc-800 text-zinc-400',
          icon: AlertOctagon,
          iconColor: 'text-zinc-500',
        };
      case 'TIMEOUT':
        return {
          label: 'TIMEOUT',
          bg: 'bg-rose-950/40 border-rose-900 text-rose-300',
          icon: AlertTriangle,
          iconColor: 'text-rose-400',
        };
      case 'NEEDS_REVIEW':
        return {
          label: 'NEEDS REVIEW',
          bg: 'bg-yellow-950/60 border-yellow-700/80 text-yellow-300',
          icon: HelpCircle,
          iconColor: 'text-yellow-400',
        };
      default:
        return {
          label: status,
          bg: 'bg-slate-900 border-slate-700 text-slate-300',
          icon: HelpCircle,
          iconColor: 'text-slate-400',
        };
    }
  };

  const config = getStatusConfig();
  const Icon = config.icon;

  const sizeClasses = {
    sm: 'text-[11px] px-2 py-0.5 gap-1.5 font-medium tracking-wide',
    md: 'text-xs px-2.5 py-1 gap-1.5 font-semibold tracking-wider',
    lg: 'text-sm px-3.5 py-1.5 gap-2 font-semibold tracking-wider',
  };

  const iconSizes = {
    sm: 'w-3 h-3',
    md: 'w-3.5 h-3.5',
    lg: 'w-4 h-4',
  };

  return (
    <span
      className={`inline-flex items-center rounded-md border backdrop-blur-sm select-none uppercase font-mono ${config.bg} ${sizeClasses[size]} ${className}`}
    >
      {showIcon && <Icon className={`${iconSizes[size]} ${config.iconColor} shrink-0`} />}
      <span>{config.label}</span>
    </span>
  );
};

export const FailureClassificationBadge: React.FC<{
  classification: FailureClassification;
  size?: 'sm' | 'md';
}> = ({ classification, size = 'sm' }) => {
  if (classification === 'PRODUCT_FAILURE') {
    return (
      <span
        className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded border border-rose-800/80 bg-rose-950/50 text-rose-300 font-mono text-[11px] font-semibold`}
        title="Product Failure: Application logic or functional regression"
      >
        <ShieldAlert className="w-3 h-3 text-rose-400" />
        <span>PRODUCT DEFECT</span>
      </span>
    );
  }

  if (classification === 'INFRASTRUCTURE_FAILURE') {
    return (
      <span
        className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded border border-orange-800/80 bg-orange-950/50 text-orange-300 font-mono text-[11px] font-semibold`}
        title="Infrastructure Failure: Network, runner timeout, or environment issue"
      >
        <ServerCrash className="w-3 h-3 text-orange-400" />
        <span>INFRA ERROR</span>
      </span>
    );
  }

  return (
    <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded border border-zinc-800 bg-zinc-900 text-zinc-400 font-mono text-[11px]">
      <span>UNCLASSIFIED</span>
    </span>
  );
};
