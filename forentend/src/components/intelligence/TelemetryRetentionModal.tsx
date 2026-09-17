/**
 * Phase 8: Telemetry Retention Modal
 * Admin-scoped bounded telemetry retention maintenance and dry-run simulation.
 */

import React, { useState } from 'react';
import {
  X,
  Trash2,
  ShieldCheck,
  AlertTriangle,
  Play,
  RotateCw,
  Clock,
  CheckCircle2,
  Database,
  Info,
} from 'lucide-react';
import type { RetentionReport } from '../../types/contract';
import { aiqaApi } from '../../services/aiqaApi';

interface TelemetryRetentionModalProps {
  projectId: string;
  isOpen: boolean;
  onClose: () => void;
}

export const TelemetryRetentionModal: React.FC<TelemetryRetentionModalProps> = ({
  projectId,
  isOpen,
  onClose,
}) => {
  const [retentionDays, setRetentionDays] = useState(30);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<RetentionReport | null>(null);

  if (!isOpen) return null;

  const handleDryRun = async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await aiqaApi.intelligence.retentionDryRun(projectId, {
        retention_days: retentionDays,
      });
      setReport(res);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Retention simulation failed');
    } finally {
      setLoading(false);
    }
  };

  const handleLivePrune = async () => {
    if (!window.confirm(`Are you sure you want to permanently prune telemetry records older than ${retentionDays} days? This action cannot be undone.`)) {
      return;
    }

    setLoading(true);
    setError(null);
    try {
      const res = await aiqaApi.intelligence.retentionPrune(projectId, {
        retention_days: retentionDays,
      });
      setReport(res);
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : 'Retention prune execution failed');
    } finally {
      setLoading(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4 bg-slate-950/80 backdrop-blur-sm overflow-y-auto"
      data-testid="telemetry-retention-modal"
    >
      <div className="bg-slate-900 border border-slate-800 rounded-2xl w-full max-w-lg shadow-2xl overflow-hidden my-auto">
        {/* Header */}
        <div className="p-5 border-b border-slate-800 flex items-center justify-between bg-slate-950/40">
          <div className="flex items-center gap-2.5">
            <Database className="w-5 h-5 text-amber-400" />
            <div>
              <h3 className="text-sm font-bold text-slate-100">Telemetry Retention Maintenance</h3>
              <p className="text-xs text-slate-400">Safe, bounded historical telemetry pruning</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded text-slate-400 hover:text-slate-100 hover:bg-slate-800 transition-colors"
            data-testid="close-retention-modal"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Content */}
        <div className="p-6 space-y-5 text-xs text-slate-300">
          {/* Safety Invariants Notice */}
          <div className="p-3.5 rounded-xl bg-slate-950/60 border border-slate-800 space-y-1.5">
            <div className="flex items-center gap-1.5 text-emerald-400 font-semibold font-mono text-[11px]">
              <ShieldCheck className="w-4 h-4" />
              <span>SAFETY INVARIANTS GUARANTEED</span>
            </div>
            <ul className="text-[11px] text-slate-400 space-y-1 list-disc list-inside">
              <li>Audit logs and Quality Graph nodes are 100% preserved.</li>
              <li>Active runs (PENDING or RUNNING) are never pruned.</li>
              <li>Telemetry linked to pending or approved healing is preserved.</li>
              <li>Prune operations are strictly tenant-isolated to this project.</li>
            </ul>
          </div>

          {/* Retention Threshold Input */}
          <div className="space-y-2">
            <label className="block text-xs font-medium text-slate-200">
              Retention Window (Days)
            </label>
            <div className="flex items-center gap-3">
              <input
                type="number"
                min="1"
                max="365"
                value={retentionDays}
                onChange={(e) => setRetentionDays(Math.max(1, parseInt(e.target.value, 10) || 1))}
                className="w-24 px-3 py-1.5 bg-slate-950 border border-slate-800 rounded-lg text-slate-100 font-mono focus:outline-none focus:border-violet-500"
                data-testid="retention-days-input"
              />
              <span className="text-slate-400 text-xs">
                Records older than {retentionDays} days will be eligible for pruning.
              </span>
            </div>
          </div>

          {/* Error Message */}
          {error && (
            <div className="p-3 bg-rose-950/40 border border-rose-800 rounded-lg text-rose-300 text-xs flex items-center gap-2">
              <AlertTriangle className="w-4 h-4 text-rose-400 flex-shrink-0" />
              <span>{error}</span>
            </div>
          )}

          {/* Report Display */}
          {report && (
            <div
              className={`p-4 rounded-xl border space-y-2 font-mono text-xs ${
                report.dry_run
                  ? 'bg-amber-950/20 border-amber-800/60 text-amber-200'
                  : 'bg-emerald-950/20 border-emerald-800/60 text-emerald-200'
              }`}
              data-testid="retention-report"
            >
              <div className="flex items-center justify-between font-bold">
                <span>
                  {report.dry_run ? 'SIMULATION OUTCOME (DRY-RUN)' : 'PRUNE EXECUTED'}
                </span>
                <span className="text-[10px] text-slate-400">
                  {new Date(report.executed_at).toLocaleTimeString()}
                </span>
              </div>
              <div className="grid grid-cols-2 gap-2 text-[11px] pt-1">
                <div>
                  <span className="text-slate-400">Eligible Records:</span>{' '}
                  <strong className="text-slate-100">{report.eligible_count}</strong>
                </div>
                <div>
                  <span className="text-slate-400">Pruned Records:</span>{' '}
                  <strong className="text-slate-100">{report.pruned_count}</strong>
                </div>
              </div>
              <p className="text-[10px] text-slate-400 font-sans mt-1">
                {report.dry_run
                  ? 'No rows deleted. Click "Execute Prune" below to apply changes permanently.'
                  : 'Selected historical telemetry rows have been permanently pruned.'}
              </p>
            </div>
          )}

          {/* Action Buttons */}
          <div className="flex items-center justify-end gap-3 pt-2">
            <button
              onClick={handleDryRun}
              disabled={loading}
              className="px-3.5 py-2 bg-slate-800 hover:bg-slate-700 text-slate-200 rounded-lg font-medium text-xs transition-colors flex items-center gap-1.5 disabled:opacity-50"
              data-testid="retention-dry-run-button"
            >
              <RotateCw className={`w-3.5 h-3.5 ${loading ? 'animate-spin' : ''}`} />
              <span>Simulate (Dry-Run)</span>
            </button>
            <button
              onClick={handleLivePrune}
              disabled={loading}
              className="px-3.5 py-2 bg-rose-900/60 hover:bg-rose-800 text-rose-200 border border-rose-700/60 rounded-lg font-medium text-xs transition-colors flex items-center gap-1.5 disabled:opacity-50"
              data-testid="retention-prune-button"
            >
              <Trash2 className="w-3.5 h-3.5 text-rose-300" />
              <span>Execute Prune</span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
