/**
 * Phase 7: Telemetry Table Section
 * Filterable, paginated real execution telemetry view with detail inspection drawer.
 */

import React, { useState, useMemo } from 'react';
import { Search, Filter, ChevronLeft, ChevronRight, Eye, AlertOctagon, CheckCircle2, Clock } from 'lucide-react';
import type { ExecutionTelemetryRow } from '../../types/contract';

interface TelemetryTableSectionProps {
  telemetry: ExecutionTelemetryRow[];
  onSelectTelemetry: (row: ExecutionTelemetryRow) => void;
  loading?: boolean;
}

export const TelemetryTableSection: React.FC<TelemetryTableSectionProps> = ({
  telemetry,
  onSelectTelemetry,
  loading,
}) => {
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState('ALL');
  const [envFilter, setEnvFilter] = useState('ALL');
  const [page, setPage] = useState(1);
  const pageSize = 10;

  // Extract unique environments
  const environments = useMemo(() => {
    const envs = new Set<string>();
    telemetry.forEach((t) => {
      if (t.environment) envs.add(t.environment);
    });
    return Array.from(envs);
  }, [telemetry]);

  // Filtered rows
  const filtered = useMemo(() => {
    return telemetry.filter((row) => {
      const q = search.trim().toLowerCase();
      if (
        q &&
        !row.task_name.toLowerCase().includes(q) &&
        !row.task_id.toLowerCase().includes(q) &&
        !row.run_id.toLowerCase().includes(q) &&
        !(row.failure_category && row.failure_category.toLowerCase().includes(q))
      ) {
        return false;
      }
      if (statusFilter !== 'ALL' && row.status !== statusFilter) {
        return false;
      }
      if (envFilter !== 'ALL' && row.environment !== envFilter) {
        return false;
      }
      return true;
    });
  }, [telemetry, search, statusFilter, envFilter]);

  const totalPages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const currentPage = Math.min(page, totalPages);
  const pagedRows = filtered.slice((currentPage - 1) * pageSize, currentPage * pageSize);

  if (loading) {
    return (
      <div className="bg-slate-900/40 border border-border-default rounded-xl p-5 animate-pulse" data-testid="telemetry-table-loading">
        <div className="h-6 w-48 bg-slate-800 rounded mb-4" />
        <div className="space-y-2">
          {[...Array(5)].map((_, i) => (
            <div key={i} className="h-10 bg-slate-800/60 rounded" />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div className="bg-slate-900/40 border border-border-default rounded-xl p-5" data-testid="telemetry-table-section">
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3 mb-4">
        <div>
          <h3 className="text-sm font-semibold text-content-primary flex items-center gap-2">
            <span>Execution Telemetry</span>
            <span className="text-xs px-2 py-0.5 rounded-full bg-slate-800 text-content-secondary font-mono">
              {filtered.length} {filtered.length === 1 ? 'event' : 'events'}
            </span>
          </h3>
          <p className="text-xs text-content-secondary mt-0.5">Real execution traces ingested from orchestrator runs.</p>
        </div>

        {/* Filter Controls */}
        <div className="flex flex-wrap items-center gap-2">
          {/* Search Input */}
          <div className="relative min-w-[180px]">
            <Search className="w-3.5 h-3.5 absolute left-2.5 top-1/2 -translate-y-1/2 text-content-secondary" />
            <input
              type="text"
              placeholder="Search task, run, category..."
              value={search}
              onChange={(e) => {
                setSearch(e.target.value);
                setPage(1);
              }}
              className="w-full pl-8 pr-3 py-1 text-xs bg-slate-950 border border-border-default rounded-md text-content-primary placeholder-slate-500 focus:outline-none focus:border-violet-500"
              data-testid="telemetry-search-input"
            />
          </div>

          {/* Status Filter */}
          <select
            value={statusFilter}
            onChange={(e) => {
              setStatusFilter(e.target.value);
              setPage(1);
            }}
            className="text-xs bg-slate-950 border border-border-default rounded-md px-2 py-1 text-content-secondary focus:outline-none focus:border-violet-500"
            data-testid="telemetry-status-filter"
          >
            <option value="ALL">All Statuses</option>
            <option value="PASSED">PASSED</option>
            <option value="FAILED">FAILED</option>
            <option value="TIMEOUT">TIMEOUT</option>
            <option value="INFRA_ERROR">INFRA_ERROR</option>
          </select>

          {/* Env Filter */}
          {environments.length > 0 && (
            <select
              value={envFilter}
              onChange={(e) => {
                setEnvFilter(e.target.value);
                setPage(1);
              }}
              className="text-xs bg-slate-950 border border-border-default rounded-md px-2 py-1 text-content-secondary focus:outline-none focus:border-violet-500"
              data-testid="telemetry-env-filter"
            >
              <option value="ALL">All Environments</option>
              {environments.map((env) => (
                <option key={env} value={env}>
                  {env}
                </option>
              ))}
            </select>
          )}
        </div>
      </div>

      {/* Table */}
      {filtered.length === 0 ? (
        <div className="py-12 text-center text-content-secondary text-xs border border-dashed border-border-default/80 rounded-lg" data-testid="telemetry-empty">
          No execution telemetry found matching the criteria.
        </div>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs text-content-secondary">
            <thead className="bg-slate-950/60 text-content-secondary uppercase font-mono text-[10px] border-b border-border-default">
              <tr>
                <th className="py-2.5 px-3">Task / ID</th>
                <th className="py-2.5 px-3">Status</th>
                <th className="py-2.5 px-3">Duration</th>
                <th className="py-2.5 px-3">Environment</th>
                <th className="py-2.5 px-3">Category</th>
                <th className="py-2.5 px-3">Start Time</th>
                <th className="py-2.5 px-3 text-right">Action</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-800/60 font-sans">
              {pagedRows.map((row) => {
                const isFail = ['FAILED', 'TIMEOUT', 'INFRA_ERROR'].includes(row.status);
                return (
                  <tr
                    key={row.id}
                    className="hover:bg-slate-800/30 transition-colors group cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-violet-400"
                    onClick={() => onSelectTelemetry(row)}
                    // Same as the flakiness table: this row is the only route to
                    // the telemetry detail modal, so it must not be mouse-only.
                    // WCAG 2.1.1 Keyboard (A).
                    tabIndex={0}
                    role="button"
                    aria-label={`Show telemetry details for ${row.task_name}`}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault();
                        onSelectTelemetry(row);
                      }
                    }}
                    data-testid={`telemetry-row-${row.id}`}
                  >
                    <td className="py-2.5 px-3">
                      <div className="font-medium text-content-primary group-hover:text-violet-300 transition-colors">
                        {row.task_name}
                      </div>
                      <div className="text-[10px] font-mono text-content-muted truncate max-w-[200px]" title={row.task_id}>
                        {row.task_id}
                      </div>
                    </td>
                    <td className="py-2.5 px-3">
                      <span
                        className={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-[11px] font-mono font-medium ${
                          row.status === 'PASSED'
                            ? 'bg-emerald-500/10 text-emerald-400 border border-emerald-500/20'
                            : isFail
                            ? 'bg-rose-500/10 text-rose-400 border border-rose-500/20'
                            : 'bg-amber-500/10 text-amber-400 border border-amber-500/20'
                        }`}
                      >
                        {row.status === 'PASSED' ? (
                          <CheckCircle2 className="w-3 h-3" />
                        ) : isFail ? (
                          <AlertOctagon className="w-3 h-3" />
                        ) : (
                          <Clock className="w-3 h-3" />
                        )}
                        {row.status}
                      </span>
                    </td>
                    <td className="py-2.5 px-3 font-mono text-content-secondary">{row.duration_ms} ms</td>
                    <td className="py-2.5 px-3 font-mono text-content-secondary">{row.environment}</td>
                    <td className="py-2.5 px-3 font-mono text-content-secondary">
                      {row.failure_category ? (
                        <span className="text-rose-300 bg-rose-950/40 px-1.5 py-0.5 rounded border border-rose-800/40">
                          {row.failure_category}
                        </span>
                      ) : (
                        <span className="text-content-muted">—</span>
                      )}
                    </td>
                    <td className="py-2.5 px-3 font-mono text-[11px] text-content-secondary">
                      {new Date(row.start_time).toLocaleString()}
                    </td>
                    <td className="py-2.5 px-3 text-right">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          onSelectTelemetry(row);
                        }}
                        className="p-1 rounded bg-slate-800 hover:bg-slate-700 text-content-secondary hover:text-content-primary transition-colors"
                        title="View Details"
                        data-testid={`view-telemetry-${row.id}`}
                      >
                        <Eye className="w-3.5 h-3.5" />
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}

      {/* Pagination */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between mt-4 pt-3 border-t border-border-default text-xs text-content-secondary">
          <div>
            Showing {(currentPage - 1) * pageSize + 1} to {Math.min(currentPage * pageSize, filtered.length)} of{' '}
            {filtered.length}
          </div>
          <div className="flex items-center gap-1">
            <button
              onClick={() => setPage((p) => Math.max(1, p - 1))}
              disabled={currentPage === 1}
              className="p-1 rounded bg-slate-800 hover:bg-slate-700 disabled:opacity-40 disabled:cursor-not-allowed text-content-secondary transition-colors"
              title="Previous Page"
            >
              <ChevronLeft className="w-4 h-4" />
            </button>
            <span className="px-2 font-mono">
              {currentPage} / {totalPages}
            </span>
            <button
              onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
              disabled={currentPage === totalPages}
              className="p-1 rounded bg-slate-800 hover:bg-slate-700 disabled:opacity-40 disabled:cursor-not-allowed text-content-secondary transition-colors"
              title="Next Page"
            >
              <ChevronRight className="w-4 h-4" />
            </button>
          </div>
        </div>
      )}
    </div>
  );
};
