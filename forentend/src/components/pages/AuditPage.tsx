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
import { FolderOpen, Inbox, RotateCcw, ScrollText, Search } from 'lucide-react';
import { ApiError, aiqaApi, getApiKey } from '../../services/aiqaApi';
import { useProject } from '../../context/ProjectContext';
import { useRealtime } from '../../context/RealtimeContext';
import { StateCard, renderResourceErrorState } from '../state/StateViews';
import { EmptyState } from '../common/EmptyState';
import type { AuditRow } from '../../types/contract';

/** FE-9: no real event type "is" an audit event (`domain.rs::EventType` has
 *  no such variant — audit rows are written by a separate `audit(...)` call
 *  inside the same request handlers, not published to the WS stream at
 *  all). But every auditable run action this page can show
 *  (create/approve/reject/cancel) genuinely produces a correlated real
 *  `RunEvent` on this project's stream shortly after (`RUN_SCHEDULED` once
 *  approval lets a run start; `RUN_COMPLETED` for a reject or a finished
 *  run; `TASK_CANCELLED` during a cancellation) — so "a real event arrived
 *  for this project" is used as the honest, disclosed trigger to
 *  invalidate/refetch the real audit resource (brief §21), not a
 *  fabricated local audit record and not a new audit-specific event this
 *  backend does not actually emit. */
const AUDIT_EVENT_RESYNC_DEBOUNCE_MS = 500;

/** Page size choices for the real page-through control below. Values
 *  chosen as reasonable round numbers within the backend's own 1..1000
 *  clamp, not derived from any document. Real pagination now exists
 *  (`api.rs::AuditLogResponse`'s real `offset`/`total`) — this is no
 *  longer, as it once honestly was, this sprint's entire pagination
 *  story. */
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
  const realtime = useRealtime();

  // `null` = not yet loaded for the current scope (or no scope at all);
  // `[]` = a real, successful fetch that genuinely returned zero rows.
  // These are deliberately distinct (brief §7/§18: "empty" is not "error"
  // and must never be conflated with "not yet requested").
  const [rows, setRows] = useState<AuditRow[] | null>(null);
  // Real total row count (`api.rs::AuditLogResponse.total`, a real
  // `COUNT(*)`) for the currently-selected project — `null` until the
  // first real response arrives, same "not yet proven" reasoning `rows`
  // itself already follows.
  const [total, setTotal] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [limit, setLimit] = useState<number>(100);
  // Real pagination offset — the real window into the real backend total
  // above, not a client-side slice of an already-fetched larger batch.
  const [offset, setOffset] = useState(0);
  const [search, setSearch] = useState('');
  const abortRef = useRef<AbortController | null>(null);

  const fetchAudit = useCallback(
    async (projectId: string, currentLimit: number, currentOffset: number, isRefresh: boolean) => {
      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;
      if (isRefresh) setRefreshing(true);
      else setLoading(true);
      setError(null);
      try {
        // `offset` omitted entirely at 0 (not sent as a redundant `offset=0`)
        // — same "never send a query param that isn't doing real work"
        // convention `limit` itself already follows when omitted.
        const result = await aiqaApi.audit.list(
          projectId,
          { limit: currentLimit, offset: currentOffset > 0 ? currentOffset : undefined },
          controller.signal,
        );
        if (controller.signal.aborted) return;
        setRows(result.rows);
        setTotal(result.total);
      } catch (err) {
        if (err instanceof DOMException && err.name === 'AbortError') return;
        const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
        setError(apiErr);
        // A failed fetch means we cannot currently prove what the real
        // audit trail looks like — never leave a stale/fabricated list
        // (or a stale real total) sitting next to an error (same rule
        // FE-5's `ProjectContext` uses).
        setRows(null);
        setTotal(null);
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
      setTotal(null);
      setError(null);
      setLoading(false);
      setRefreshing(false);
      setOffset(0);
      return;
    }
    // Project switch (or a limit change, which is a genuinely different
    // query): never let the previous project's rows keep rendering while
    // the new scope loads (brief §16 — "old events do not remain
    // incorrectly visible"), and always land back on the real first page —
    // a stale offset from the previous scope/page size could point past
    // the new real total.
    setRows(null);
    setOffset(0);
    void fetchAudit(selectedProjectId, limit, 0, false);
  }, [selectedProjectId, limit, fetchAudit]);

  useEffect(() => () => abortRef.current?.abort(), []);

  const hasPrevPage = offset > 0;
  const hasNextPage = total === null ? false : offset + limit < total;

  const handlePrevPage = useCallback(() => {
    if (!selectedProjectId || loading || refreshing || !hasPrevPage) return;
    const newOffset = Math.max(0, offset - limit);
    setOffset(newOffset);
    // A page turn is a real fetch for real new rows, not a "same data,
    // background resync" — but it reuses the refresh (not full-loading)
    // rendering path so the currently-visible page stays on screen while
    // the new one loads, same "no jarring rows-to-loading-card flash"
    // reasoning `handleRefresh` below already established.
    void fetchAudit(selectedProjectId, limit, newOffset, true);
  }, [selectedProjectId, limit, offset, loading, refreshing, hasPrevPage, fetchAudit]);

  const handleNextPage = useCallback(() => {
    if (!selectedProjectId || loading || refreshing || !hasNextPage) return;
    const newOffset = offset + limit;
    setOffset(newOffset);
    void fetchAudit(selectedProjectId, limit, newOffset, true);
  }, [selectedProjectId, limit, offset, loading, refreshing, hasNextPage, fetchAudit]);

  // FE-9: a real event on this project's stream is treated as "the audit
  // trail may have changed" (see the module doc comment above for exactly
  // why) — invalidates/refetches via the *existing* `fetchAudit`, never a
  // second audit-fetching path. Debounced so a burst of task events from
  // one active run collapses into a single real request, not one per
  // event. Uses `isRefresh: true` (same as the manual Refresh button) so a
  // background resync never blanks already-rendered rows with a full
  // loading state, and resyncs the real page currently being viewed
  // (`offset`) rather than silently jumping the user back to page 1.
  const auditResyncTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (!selectedProjectId) return;
    const unsubscribe = realtime.subscribe(() => {
      if (auditResyncTimeoutRef.current) clearTimeout(auditResyncTimeoutRef.current);
      auditResyncTimeoutRef.current = setTimeout(() => {
        auditResyncTimeoutRef.current = null;
        void fetchAudit(selectedProjectId, limit, offset, true);
      }, AUDIT_EVENT_RESYNC_DEBOUNCE_MS);
    });
    return () => {
      unsubscribe();
      if (auditResyncTimeoutRef.current) clearTimeout(auditResyncTimeoutRef.current);
    };
    // Live-discovered FE-9 bug fix (see `ExecutionHubPage.tsx`'s own,
    // fuller comment on this exact bug): depend on the stable
    // `realtime.subscribe` identity, not the whole `realtime` object
    // (which changes on every single event via `lastEventAt`) — otherwise
    // this effect tears down and re-subscribes on every event, cancelling
    // its own just-scheduled debounce timer before it can fire.
  }, [selectedProjectId, limit, offset, realtime.subscribe, fetchAudit]);

  const handleRefresh = useCallback(() => {
    // A plain refresh of the *same* scope + real page keeps the
    // currently-rendered rows visible (only a small spinner on the button
    // changes) — unlike a project switch, nothing here has been shown to
    // be stale (brief §17).
    if (!selectedProjectId || loading || refreshing) return;
    void fetchAudit(selectedProjectId, limit, offset, true);
  }, [selectedProjectId, limit, offset, loading, refreshing, fetchAudit]);

  const visibleRows = useMemo(() => {
    if (!rows) return [];
    return rows.map((row, index) => ({ row, index })).filter(({ row }) => matchesSearch(row, search));
  }, [rows, search]);

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
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
          title="Loading audit trail"
          description={`Fetching recorded actions for ${selectedProject?.name ?? 'this project'}…`}
        />
      );
    }

    if (error) {
      return renderResourceErrorState(error, handleRefresh, {
        forbiddenTitle: 'Not permitted to view this audit trail',
        forbiddenDescription: `This key's role or project scope does not permit viewing ${selectedProject?.name ?? 'this project'}'s audit trail (the backend requires Operator role or above). This is a permission problem, not a bad credential.`,
        offlineDescriptionSuffix: 'to load the audit trail.',
        genericTitle: "Couldn't load the audit trail",
      });
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
                <td className="py-2.5 pr-3 font-mono text-[10px] text-violet-400">{row.role ?? '—'}</td>
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
            <span className="p-1.5 rounded bg-violet-950/80 border border-violet-700/60 text-violet-400">
              <ScrollText className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-slate-100 font-sans tracking-tight">Audit Trail</h1>
            <span className="px-2 py-0.5 rounded bg-violet-950/60 border border-violet-800/60 text-[10px] font-mono text-violet-300 uppercase">
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
            className="rounded-md border border-slate-800 bg-[#020617] hover:bg-slate-900 px-2.5 py-1.5 text-xs text-slate-300 outline-none focus:ring-1 focus:ring-violet-500 font-mono cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
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
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 font-medium text-xs border border-slate-700 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-400"
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
              className="w-full pl-8 pr-3 py-1.5 rounded-md border border-slate-800 bg-[#020617] text-xs text-slate-200 outline-none focus:ring-1 focus:ring-violet-500 font-sans"
            />
          </div>
          {/* Real, disclosed scope limit — the backend has no server-side
              search, so this only ever filters the current real page's own
              already-fetched rows, never every page at once. */}
          <span className="hidden sm:inline text-[10px] font-mono text-slate-500 shrink-0">
            (this page only)
          </span>
        </div>
      )}

      <section id="audit-content" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm" aria-live="polite">
        {renderBody()}
      </section>

      {selectedProjectId && rows && total !== null && total > 0 && (
        <div
          id="audit-pagination"
          className="flex items-center justify-between gap-3 p-3 rounded-lg border border-slate-800 bg-[#0B1120] text-xs font-mono text-slate-400"
        >
          <span>
            Showing {offset + 1}-{Math.min(offset + limit, total)} of {total}
          </span>
          <div className="flex items-center gap-2">
            <button
              type="button"
              id="audit-prev-page"
              onClick={handlePrevPage}
              disabled={!hasPrevPage || loading || refreshing}
              className="px-2.5 py-1 rounded border border-slate-700 hover:border-violet-600 hover:text-violet-300 disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:border-slate-700 disabled:hover:text-slate-400 cursor-pointer transition-colors"
            >
              Previous
            </button>
            <button
              type="button"
              id="audit-next-page"
              onClick={handleNextPage}
              disabled={!hasNextPage || loading || refreshing}
              className="px-2.5 py-1 rounded border border-slate-700 hover:border-violet-600 hover:text-violet-300 disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:border-slate-700 disabled:hover:text-slate-400 cursor-pointer transition-colors"
            >
              Next
            </button>
          </div>
        </div>
      )}
    </div>
  );
};
