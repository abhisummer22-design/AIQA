/**
 * Phase 7: Telemetry Detail Drawer / Modal
 * Displays complete operational metadata of an individual execution telemetry record.
 */

import React from 'react';
import { X, ShieldCheck, Clock, Terminal, AlertOctagon, CheckCircle2, Hash, Layers } from 'lucide-react';
import type { ExecutionTelemetryRow } from '../../types/contract';

interface TelemetryDetailModalProps {
  telemetry: ExecutionTelemetryRow | null;
  onClose: () => void;
}

function sanitizeErrorMessage(msg: string): string {
  return msg
    .replace(/aiqa_[a-zA-Z0-9_\-]+/g, '••••••••')
    .replace(/(bearer\s+)[a-zA-Z0-9_\-\.]+/gi, '$1••••••••')
    .replace(/(token[:=\s]+)[a-zA-Z0-9_\-\.]+/gi, '$1••••••••');
}

export const TelemetryDetailModal: React.FC<TelemetryDetailModalProps> = ({ telemetry, onClose }) => {
  if (!telemetry) return null;

  const isFailed = ['FAILED', 'TIMEOUT', 'INFRA_ERROR'].includes(telemetry.status);

  return (
    <div
      className="fixed inset-0 z-50 bg-slate-950/80 backdrop-blur-sm flex items-center justify-center p-4"
      data-testid="telemetry-detail-modal"
    >
      <div className="bg-surface-card border border-border-default rounded-xl shadow-2xl max-w-2xl w-full max-h-[90vh] flex flex-col overflow-hidden animate-in fade-in zoom-in-95 duration-150">
        {/* Header */}
        <div className="p-4 border-b border-border-default flex items-center justify-between bg-slate-900/40">
          <div className="flex items-center gap-2.5">
            <div
              className={`p-1.5 rounded-md ${
                isFailed ? 'bg-rose-500/10 text-rose-400' : 'bg-emerald-500/10 text-emerald-400'
              }`}
            >
              {isFailed ? <AlertOctagon className="w-5 h-5" /> : <CheckCircle2 className="w-5 h-5" />}
            </div>
            <div>
              <h3 className="text-sm font-semibold text-content-primary">{telemetry.task_name}</h3>
              <p className="text-xs font-mono text-content-secondary">Run: {telemetry.run_id}</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded-md text-content-secondary hover:text-content-primary hover:bg-slate-800 transition-colors"
            title="Close"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="p-5 overflow-y-auto space-y-4 text-xs">
          {/* Metadata Grid */}
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-3 bg-slate-900/30 border border-border-default/80 rounded-lg p-3.5 font-mono">
            <div>
              <span className="text-[10px] text-content-muted uppercase block">Status</span>
              <span
                className={`font-semibold ${
                  telemetry.status === 'PASSED'
                    ? 'text-emerald-400'
                    : isFailed
                    ? 'text-rose-400'
                    : 'text-amber-400'
                }`}
              >
                {telemetry.status}
              </span>
            </div>
            <div>
              <span className="text-[10px] text-content-muted uppercase block">Executor</span>
              <span className="text-content-primary">{telemetry.executor}</span>
            </div>
            <div>
              <span className="text-[10px] text-content-muted uppercase block">Environment</span>
              <span className="text-content-primary">{telemetry.environment}</span>
            </div>
            <div>
              <span className="text-[10px] text-content-muted uppercase block">Duration</span>
              <span className="text-content-primary">{telemetry.duration_ms} ms</span>
            </div>
            <div>
              <span className="text-[10px] text-content-muted uppercase block">Retries</span>
              <span className="text-content-primary">{telemetry.retry_count}</span>
            </div>
            <div>
              <span className="text-[10px] text-content-muted uppercase block">Healing Attempts</span>
              <span className="text-content-primary">{telemetry.healing_attempt_count}</span>
            </div>
            <div>
              <span className="text-[10px] text-content-muted uppercase block">Task ID</span>
              <span className="text-content-primary truncate block" title={telemetry.task_id}>
                {telemetry.task_id}
              </span>
            </div>
            <div>
              <span className="text-[10px] text-content-muted uppercase block">HTTP Status</span>
              <span className="text-content-primary">{telemetry.http_status ?? 'N/A'}</span>
            </div>
            <div>
              <span className="text-[10px] text-content-muted uppercase block">Verification</span>
              <span className="text-content-primary">{telemetry.verification_outcome ?? 'None'}</span>
            </div>
          </div>

          {/* Failure Intelligence Details */}
          {isFailed && (
            <div className="space-y-3">
              <div className="bg-rose-500/5 border border-rose-500/20 rounded-lg p-3">
                <div className="flex items-center gap-2 mb-1.5 text-rose-400 font-semibold">
                  <AlertOctagon className="w-4 h-4" />
                  <span>Failure Category: {telemetry.failure_category ?? 'UNKNOWN'}</span>
                </div>
                {telemetry.failure_fingerprint && (
                  <div className="font-mono text-[11px] text-content-secondary break-all">
                    Fingerprint: <span className="text-content-primary">{telemetry.failure_fingerprint}</span>
                  </div>
                )}
                {telemetry.selector && (
                  <div className="mt-1 font-mono text-[11px] text-amber-400">
                    Selector: <code className="bg-slate-950 px-1.5 py-0.5 rounded">{telemetry.selector}</code>
                  </div>
                )}
              </div>

              {telemetry.error_message && (
                <div>
                  <span className="text-xs font-semibold text-content-secondary block mb-1">Sanitized Error Message</span>
                  <pre className="bg-slate-950 border border-border-default rounded-lg p-3 font-mono text-[11px] text-rose-300 whitespace-pre-wrap overflow-x-auto max-h-40">
                    {sanitizeErrorMessage(telemetry.error_message)}
                  </pre>
                </div>
              )}
            </div>
          )}

          {/* Assertion Result */}
          {telemetry.assertion_result && (
            <div>
              <span className="text-xs font-semibold text-content-secondary block mb-1">Assertion Diagnostic</span>
              <pre className="bg-slate-950 border border-border-default rounded-lg p-3 font-mono text-[11px] text-content-secondary whitespace-pre-wrap">
                {telemetry.assertion_result}
              </pre>
            </div>
          )}

          {/* Timestamp & Privacy Notice */}
          <div className="flex items-center justify-between text-[11px] text-content-muted pt-2 border-t border-border-default/80">
            <span className="flex items-center gap-1.5">
              <Clock className="w-3.5 h-3.5" />
              Recorded: {new Date(telemetry.created_at).toLocaleString()}
            </span>
            <span className="flex items-center gap-1 text-emerald-400 font-medium">
              <ShieldCheck className="w-3.5 h-3.5" />
              Sanitized & Redacted
            </span>
          </div>
        </div>
      </div>
    </div>
  );
};
