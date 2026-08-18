/**
 * FE-6 — the real Audit Trail page.
 *
 * Backend contract (re-verified at the start of this sprint, `services/
 * orchestrator/src/api.rs::get_audit_log_scoped`, `store.rs::AuditRow`/
 * `list_audit`): `GET /api/v1/projects/:project_id/audit`, gated by
 * `Role::Operator` + `require_scoped_project` (so a viewer-role or
 * wrong-project key genuinely 403s here — unlike `AuthContext`'s own
 * `/projects` validation call, which structurally never can, per that
 * file's own doc comment). Query support is exactly `limit` (i64, server
 * clamps 1..1000) — no date range, actor/action/resource filter, sort
 * field, or page/offset/cursor exists on the backend today. Response rows
 * (`AuditRow`): `occurred_at, actor, role, project_id, action, target,
 * detail, outcome` — no `id`, no `request_id` (that only exists on
 * `ApiError` envelopes, shown separately in the error state below).
 * Already-real, already-typed via FE-1 (`aiqaApi.audit.list`,
 * `types/contract.ts`'s `AuditRow`) — nothing new was added to either.
 *
 * No FRD/SRD/PRD/flow-diagram document exists in this repository (re-
 * confirmed at the start of this sprint, same search as every prior FE
 * unit's own report) — every behavior below is derived from the real
 * backend contract above, this sprint's own explicit constraints, and (for
 * the table's shape only, restyled to this app's own design tokens — see
 * this file's own header/card markup vs. `apps/web/src/views/
 * AuditLogView.tsx`'s `#0a0f1a`/cyan theme) the equivalent reference view
 * in `apps/web`, which is reference-only and was not copied wholesale.
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { AlertTriangle, FolderOpen, Inbox, RotateCcw, ScrollText, Search, ShieldAlert, WifiOff, XCircle } from 'lucide-react';
import { ApiError, aiqaApi, getApiKey } from '../../services/aiqaApi';
import { useProject } from '../../context/ProjectContext';
import { StateCard, Cta } from '../state/StateViews';
import { EmptyState } from '../common/EmptyState';
import type { AuditRow } from '../../types/contract';

/** The entirety of this sprint's pagination support (brief §13): the real
 *  backend exposes no cursor/offset/page, only `limit` — so "pagination"
 *  here is this one control, not an invented multi-page contract. Values
 *  chosen as reasonable round numbers within the backend's own 1..1000
 *  clamp, not derived from any document. */
const LIMIT_OPTIONS = [50, 100, 250, 500] as const;

/** Client-side only, over already-returned rows — never a backend query
 *  parameter (brief §11 rule 3: "a clearly safe client-side derivation from
 *  already-returned fields"). Mirrors `apps/web`'s own reference view. */
function matchesSearch(row: AuditRow, query: string): boolean {
  if (!query) return true;
  const q = query.toLowerCase();
  return [row.actor, row.role, row.action, row.target, row.detail, row.outcome, row.project_id]
    .filter((v): v is string => Boolean(v))
    .some((v) => v.toLowerCase().includes(q));
}

function formatTimestamp(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    year: 'numeric',
    month: 'short',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

/**
 * Defensive-in-depth (brief §14: "even if backend metadata accidentally
 * contains sensitive-looking fields, handle them safely and do not render
 * credentials"). This page never reads or displays the user's key by
 * design (unlike Settings, nothing here calls `maskApiKey`) — this only
 * guards the narrow, real residual risk: a backend-returned `actor`/
 * `target`/`detail` string that happens to *contain* the exact raw key
 * this browser currently holds (e.g. a hypothetical future audit action
 * that over-logged its own credential). A broad "looks like a secret"
 * heuristic would risk hiding legitimate audit content instead, so this
 * redacts only the one string this browser can actually prove is
 * sensitive: its own currently-configured key. */
function redactOwnKey(value: string | null): string | null {
  if (!value) return value;
  const key = getApiKey();
  if (!key) return value;
  return value.split(key).join('••••••••');
}

const outcomeBadgeClass = (outcome: string): string =>
  outcome === 'ok'
    ? 'bg-emerald-950/80 text-emerald-300 border-emerald-800/80'
    : 'bg-rose-950/80 text-rose-300 border-rose-800/80';

export const AuditPage: React.FC = () => {
  const { selectedProjectId, selectedProject } = useProject();

  // `null` = not yet loaded for the current scope (or no scope at all);
  // `[]` = a real, successful fetch that genuinely returned zero rows.
  // These are deliberately distinct (brief §7/§18: "empty" is not "error"
  // and must never be conflated with "not yet requested").
  const [rows, setRows] = useState<AuditRow[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [limit, setLimit] = useState<number>(100);
  const [search, setSearch] = useState('');
  const abortRef = useRef<AbortController | null>(null);

  const fetchAudit = useCallback(
    async (projectId: string, currentLimit: number, isRefresh: boolean) => {
      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;
      if (isRefresh) setRefreshing(true);
      else setLoading(true);
      setError(null);
      try {
        const list = await aiqaApi.audit.list(projectId, { limit: currentLimit }, controller.signal);
        if (controller.signal.aborted) return;
        setRows(list);
      } catch (err) {
        if (err instanceof DOMException && err.name === 'AbortError') return;
        const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
        setError(apiErr);
        // A failed fetch means we cannot currently prove what the real
        // audit trail looks like — never leave a stale/fabricated list
        // sitting next to an error (same rule FE-5's `ProjectContext` uses).
        setRows(null);
      } finally {
        if (!controller.signal.aborted) {
          setLoading(false);
          setRefreshing(false);
        }
      }
    },
    [],
  );

  useEffect(() => {
    if (!selectedProjectId) {
      // No real project scope → the real endpoint (`/projects/:id/audit`)
      // cannot even be constructed. Never fabricate scoped records here
      // (brief §5) — this is a distinct, honest "nothing to show yet"
      // state, not an error and not a fetch attempt.
      abortRef.current?.abort();
      setRows(null);
      setError(null);
      setLoading(false);
      setRefreshing(false);
      return;
    }
    // Project switch (or a limit change, which is a genuinely different
    // query): never let the previous project's rows keep rendering while
    // the new scope loads (brief §16 — "old events do not remain
    // incorrectly visible").
    setRows(null);
    void fetchAudit(selectedProjectId, limit, false);
  }, [selectedProjectId, limit, fetchAudit]);

  useEffect(() => () => abortRef.current?.abort(), []);

  const handleRefresh = useCallback(() => {
    // A plain refresh of the *same* scope keeps the currently-rendered
    // rows visible (only a small spinner on the button changes) — unlike a
    // project switch, nothing here has been shown to be stale (brief §17).
    if (!selectedProjectId || loading || refreshing) return;
    void fetchAudit(selectedProjectId, limit, true);
  }, [selectedProjectId, limit, loading, refreshing, fetchAudit]);

  const visibleRows = useMemo(() => {
    if (!rows) return [];
    return rows.map((row, index) => ({ row, index })).filter(({ row }) => matchesSearch(row, search));
  }, [rows, search]);

  const requestIdNode = error?.requestId ? (
    <p className="text-[10px] font-mono text-slate-500">request_id: {error.requestId}</p>
  ) : null;

  const renderBody = () => {
    if (!selectedProjectId) {
      return (
        <EmptyState
          icon={FolderOpen}
          title="No project selected"
          description="Choose a project from the selector in the header to view its audit trail."
        />
      );
    }

    if (loading) {
      return (
        <StateCard
          icon={ScrollText}
          iconClassName="text-indigo-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-indigo-300 bg-indigo-950/60 border-indigo-800"
          title="Loading audit trail"
          description={`Fetching recorded actions for ${selectedProject?.name ?? 'this project'}…`}
        />
      );
    }

    if (error) {
      switch (error.kind) {
        case 'unauthorized':
          return (
            <StateCard
              icon={XCircle}
              iconClassName="text-rose-400"
              statusLabel="AUTHENTICATION REQUIRED · 401"
              statusClassName="text-rose-400 bg-rose-950/60 border-rose-800"
              title="Authentication required"
              description="The orchestrator rejected this request. The API key may have been revoked since it was last verified."
              actions={<Cta label="Retry" icon={RotateCcw} onClick={handleRefresh} primary />}
            >
              {requestIdNode}
            </StateCard>
          );
        case 'forbidden':
          return (
            <StateCard
              icon={ShieldAlert}
              iconClassName="text-orange-400"
              statusLabel="ACCESS FORBIDDEN · 403"
              statusClassName="text-orange-400 bg-orange-950/60 border-orange-800"
              title="Not permitted to view this audit trail"
              description={`This key's role or project scope does not permit viewing ${selectedProject?.name ?? 'this project'}'s audit trail (the backend requires Operator role or above). This is a permission problem, not a bad credential.`}
              actions={<Cta label="Retry" icon={RotateCcw} onClick={handleRefresh} primary />}
            >
              {requestIdNode}
            </StateCard>
          );
        case 'offline':
          return (
            <StateCard
              icon={WifiOff}
              iconClassName="text-slate-400"
              statusLabel="ORCHESTRATOR UNREACHABLE"
              statusClassName="text-slate-400 bg-slate-800/60 border-slate-700"
              title="Orchestrator unreachable"
              description="Unable to reach the AIQA Orchestrator to load the audit trail."
              actions={<Cta label="Retry" icon={RotateCcw} onClick={handleRefresh} primary />}
            >
              {requestIdNode}
            </StateCard>
          );
        default:
          return (
            <StateCard
              icon={AlertTriangle}
              iconClassName="text-rose-400"
              statusLabel="ERROR"
              statusClassName="text-rose-400 bg-rose-950/60 border-rose-800"
              title="Couldn't load the audit trail"
              description={error.message}
              actions={<Cta label="Retry" icon={RotateCcw} onClick={handleRefresh} primary />}
            >
              {requestIdNode}
            </StateCard>
          );
      }
    }

    if (rows && rows.length === 0) {
      return (
        <EmptyState
          icon={Inbox}
          title="No audit events recorded"
          description={`The orchestrator has recorded no auditable actions for ${selectedProject?.name ?? 'this project'} yet.`}
        />
      );
    }

    if (rows && visibleRows.length === 0) {
      return (
        <div className="py-8 text-center text-xs text-slate-500 font-mono">
          No entries match &ldquo;{search}&rdquo;. ({rows.length} total loaded.)
        </div>
      );
    }

    return (
      <div className="overflow-x-auto">
        <table className="w-full min-w-[860px] text-left text-xs">
          <thead>
            <tr className="font-mono text-[10px] uppercase tracking-wider text-slate-500 border-b border-slate-800/80">
              <th scope="col" className="pb-2.5 pr-3 font-normal">Timestamp</th>
              <th scope="col" className="pb-2.5 pr-3 font-normal">Actor</th>
              <th scope="col" className="pb-2.5 pr-3 font-normal">Role</th>
              <th scope="col" className="pb-2.5 pr-3 font-normal">Action</th>
              <th scope="col" className="pb-2.5 pr-3 font-normal">Target</th>
              <th scope="col" className="pb-2.5 pr-3 font-normal">Detail</th>
              <th scope="col" className="pb-2.5 font-normal">Outcome</th>
            </tr>
          </thead>
          <tbody>
            {visibleRows.map(({ row, index }) => (
              <tr key={`${row.occurred_at}-${index}`} className="border-t border-slate-800/60 align-top hover:bg-slate-900/40 transition-colors">
                <td className="whitespace-nowrap py-2.5 pr-3 font-mono text-[11px] text-slate-300">
                  {formatTimestamp(row.occurred_at)}
                </td>
                <td className="py-2.5 pr-3 text-slate-200 font-medium">{redactOwnKey(row.actor) ?? '—'}</td>
                <td className="py-2.5 pr-3 font-mono text-[10px] text-indigo-400">{row.role ?? '—'}</td>
                <td className="py-2.5 pr-3 font-mono text-xs font-semibold text-slate-100">{row.action}</td>
                <td className="py-2.5 pr-3 font-mono text-[10px] text-slate-400 max-w-[180px] truncate" title={redactOwnKey(row.target) ?? undefined}>
                  {redactOwnKey(row.target) ?? '—'}
                </td>
                <td className="py-2.5 pr-3 text-slate-400 max-w-sm truncate" title={redactOwnKey(row.detail) ?? undefined}>
                  {redactOwnKey(row.detail) ?? '—'}
                </td>
                <td className="py-2.5">
                  <span className={`rounded px-2 py-0.5 font-mono text-[9px] uppercase font-semibold tracking-wider border ${outcomeBadgeClass(row.outcome)}`}>
                    {row.outcome}
                  </span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  };

  return (
    <div id="aiqa-audit-page" className="space-y-4 animate-in fade-in duration-200 text-slate-200">
      <header
        id="audit-header"
        className="flex flex-col md:flex-row md:items-center justify-between gap-3 p-4 rounded-lg bg-[#0B1120] border border-slate-800/90 shadow-sm"
      >
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <span className="p-1.5 rounded bg-indigo-950/80 border border-indigo-700/60 text-indigo-400">
              <ScrollText className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-slate-100 font-sans tracking-tight">Audit Trail</h1>
            <span className="px-2 py-0.5 rounded bg-indigo-950/60 border border-indigo-800/60 text-[10px] font-mono text-indigo-300 uppercase">
              AUDIT
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans">
            {selectedProject
              ? `Recorded control-plane actions for ${selectedProject.name}`
              : 'Recorded control-plane actions for the selected project'}
          </p>
        </div>

        <div className="flex items-center gap-2 self-start md:self-center">
          <label htmlFor="audit-limit-select" className="sr-only">
            Number of entries to load
          </label>
          <select
            id="audit-limit-select"
            value={limit}
            onChange={(e) => setLimit(Number(e.target.value))}
            disabled={!selectedProjectId}
            className="rounded-md border border-slate-800 bg-[#020617] hover:bg-slate-900 px-2.5 py-1.5 text-xs text-slate-300 outline-none focus:ring-1 focus:ring-indigo-500 font-mono cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {LIMIT_OPTIONS.map((n) => (
              <option key={n} value={n}>
                Last {n}
              </option>
            ))}
          </select>
          <button
            type="button"
            id="audit-refresh-button"
            onClick={handleRefresh}
            disabled={!selectedProjectId || loading || refreshing}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 font-medium text-xs border border-slate-700 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none focus-visible:ring-2 focus-visible:ring-indigo-400"
          >
            <RotateCcw className={`w-3.5 h-3.5 ${refreshing ? 'animate-spin' : ''}`} />
            <span>Refresh</span>
          </button>
        </div>
      </header>

      {selectedProjectId && rows && rows.length > 0 && (
        <div className="flex items-center gap-2 p-2.5 rounded-lg border border-slate-800 bg-[#0B1120]">
          <div className="relative w-full max-w-sm">
            <Search className="w-3.5 h-3.5 text-slate-500 absolute left-2.5 top-1/2 -translate-y-1/2" />
            <label htmlFor="audit-search-input" className="sr-only">
              Search audit entries
            </label>
            <input
              id="audit-search-input"
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search actors, actions, targets…"
              className="w-full pl-8 pr-3 py-1.5 rounded-md border border-slate-800 bg-[#020617] text-xs text-slate-200 outline-none focus:ring-1 focus:ring-indigo-500 font-sans"
            />
          </div>
        </div>
      )}

      <section id="audit-content" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm" aria-live="polite">
        {renderBody()}
      </section>
    </div>
  );
};
