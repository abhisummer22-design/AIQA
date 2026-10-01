/**
 * Phase 6 — Requirement Coverage: what the documents say, against what the
 * application shows.
 *
 * Backend contract (`services/orchestrator/src/gap_check.rs`, `api.rs::gap_check_scoped`):
 * `POST /api/v1/projects/:project_id/gap-check` lines the project's numbered
 * requirements up with the pages of a crawl of the running application and with
 * the saved cases that test them. Each requirement is FOUND (a page carries the
 * words it is about), WEAK (something related is on a page), MISSING ("no UI
 * evidence") or NOT_CHECKABLE (a crawl cannot show a response time, an API or
 * background text); the pages no requirement points at come back as "not in the
 * documents". It is evidence that a feature has somewhere to live, not proof it
 * works — the page says so, and shows the words and elements behind every match.
 *
 * With no options it is free and needs no AI provider, so the page runs it by
 * itself whenever the project or the crawl changes. The two options that call a
 * provider (the embedder, and the router's AI checking the unclear ones) run only
 * from the button.
 *
 * Crawls are chosen from the project's real runs (`aiqaApi.executions.list`): a
 * crawl is a run whose name starts `Crawl:` — the name `api.rs::start_crawl_scoped`
 * gives it — so nothing here is invented, only composed.
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  AlertTriangle,
  Bot,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  FolderOpen,
  Inbox,
  Layers,
  ListChecks,
  Play,
  RotateCcw,
  Search,
  Sparkles,
} from 'lucide-react';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import { useProject } from '../../context/ProjectContext';
import { StateCard, renderResourceErrorState } from '../state/StateViews';
import { EmptyState } from '../common/EmptyState';
import { formatStatusLabel } from '../../utils/statusLabel';
import type {
  GapCheckRequest,
  GapCheckResponse,
  GapPageMatch,
  GapRequirement,
  GapStatus,
  RunListItem,
} from '../../types/contract';

/** The name `api.rs::start_crawl_scoped` gives a crawl's run. */
const CRAWL_RUN_PREFIX = 'Crawl:';
const DEFAULT_MAX_VERIFICATIONS = 12;
const MAX_VERIFICATIONS_LIMIT = 60;

const STATUS_ORDER: GapStatus[] = ['FOUND', 'WEAK', 'MISSING', 'NOT_CHECKABLE'];

const STATUS_META: Record<GapStatus, { label: string; badge: string; hint: string }> = {
  FOUND: {
    label: 'Found',
    badge: 'text-emerald-300 bg-emerald-950/50 border-emerald-800/70',
    hint: 'A page of the crawl carries what this requirement is about.',
  },
  WEAK: {
    label: 'Weak',
    badge: 'text-amber-300 bg-amber-950/50 border-amber-800/70',
    hint: 'Something related is on a page, but not enough to say the feature is there.',
  },
  MISSING: {
    label: 'No UI evidence',
    badge: 'text-rose-300 bg-rose-950/50 border-rose-800/70',
    hint: 'No page of the crawl carries what this requirement is about.',
  },
  NOT_CHECKABLE: {
    label: 'Not checkable',
    badge: 'text-slate-400 bg-slate-800/60 border-slate-700',
    hint: 'A crawl cannot show this kind of requirement (a response time, an API, background text).',
  },
};

const BASIS_LABEL: Record<string, string> = {
  text: 'matched on words',
  embedding: 'similar in meaning',
  ai: 'confirmed by the AI',
};

const plural = (n: number, one: string, many: string): string => (n === 1 ? one : many);

/** The address without its origin — what tells two pages of one application apart. */
function pathOf(url: string): string {
  try {
    const u = new URL(url);
    return `${u.pathname}${u.search}` || '/';
  } catch {
    return url;
  }
}

const percent = (score: number): string => `${Math.round(score * 100)}%`;

const isCrawlRun = (run: RunListItem): boolean => run.name.startsWith(CRAWL_RUN_PREFIX);

const Chip: React.FC<{ children: React.ReactNode; tone?: 'plain' | 'amber' | 'violet' }> = ({ children, tone = 'plain' }) => {
  const cls =
    tone === 'amber'
      ? 'text-amber-300 bg-amber-950/40 border-amber-800/60'
      : tone === 'violet'
        ? 'text-violet-300 bg-violet-950/40 border-violet-800/60'
        : 'text-slate-300 bg-[#020617] border-slate-800';
  return <span className={`inline-flex items-center px-1.5 py-0.5 rounded border text-[10px] font-mono ${cls}`}>{children}</span>;
};

const StatusBadge: React.FC<{ status: GapStatus }> = ({ status }) => (
  <span
    className={`inline-flex items-center px-2 py-0.5 rounded border text-[10px] font-mono uppercase tracking-wide ${STATUS_META[status].badge}`}
    title={STATUS_META[status].hint}
  >
    {STATUS_META[status].label}
  </span>
);

// ---------------------------------------------------------------- one requirement

const MatchDetail: React.FC<{ match: GapPageMatch }> = ({ match }) => (
  <li className="p-2 rounded bg-[#020617] border border-slate-800 space-y-1">
    <div className="flex flex-wrap items-center gap-2">
      <span className="font-mono text-[11px] text-slate-100 break-all">{pathOf(match.url)}</span>
      {match.title && <span className="text-[11px] text-slate-400">{match.title}</span>}
      <span className="font-mono text-[10px] text-violet-300">{percent(match.score)} of what it says</span>
      <span className="text-[10px] text-content-muted">{BASIS_LABEL[match.basis] ?? match.basis}</span>
      {match.similarity !== undefined && (
        <span className="text-[10px] text-content-muted">similarity {match.similarity.toFixed(2)}</span>
      )}
    </div>
    {match.terms.length > 0 && (
      <div className="flex flex-wrap items-center gap-1">
        <span className="text-[10px] text-content-muted">words on the page:</span>
        {match.terms.map((t) => (
          <Chip key={t} tone="violet">
            {t}
          </Chip>
        ))}
      </div>
    )}
    {match.elements.length > 0 && (
      <div className="flex flex-wrap items-center gap-1">
        <span className="text-[10px] text-content-muted">carried by:</span>
        {match.elements.map((e) => (
          <Chip key={e}>{e}</Chip>
        ))}
      </div>
    )}
    {match.roles.length > 0 && <div className="text-[10px] text-content-muted">reached by: {match.roles.join(', ')}</div>}
  </li>
);

const RequirementRow: React.FC<{
  id: string;
  requirement: GapRequirement;
  open: boolean;
  onToggle: () => void;
}> = ({ id, requirement: r, open, onToggle }) => {
  const best = r.matches[0];
  const techniques = Object.entries(r.cases.techniques);
  return (
    <li id={id} className="border-b border-slate-800/80 last:border-b-0">
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        className="w-full grid grid-cols-[auto_1fr] md:grid-cols-[auto_minmax(0,1fr)_auto_minmax(0,14rem)_auto] items-start gap-x-3 gap-y-1 py-2.5 text-left hover:bg-slate-900/40 transition-colors cursor-pointer focus:outline-none focus-visible:ring-1 focus-visible:ring-violet-400 rounded"
      >
        {open ? (
          <ChevronDown className="w-3.5 h-3.5 mt-0.5 text-content-muted shrink-0" />
        ) : (
          <ChevronRight className="w-3.5 h-3.5 mt-0.5 text-content-muted shrink-0" />
        )}
        <span className="min-w-0">
          <span className="flex items-center gap-2">
            <span className="font-mono text-xs font-semibold text-slate-100">{r.key}</span>
            <Chip>{r.kind}</Chip>
          </span>
          <span className="block text-xs text-slate-300 line-clamp-2">{r.text}</span>
        </span>
        <span className="flex flex-col items-start gap-1">
          <StatusBadge status={r.status} />
          {r.basis === 'ai' && (
            <span className="inline-flex items-center gap-1 text-[10px] text-violet-300">
              <Bot className="w-3 h-3" /> AI-checked
            </span>
          )}
          {r.basis === 'embedding' && (
            <span className="inline-flex items-center gap-1 text-[10px] text-violet-300">
              <Sparkles className="w-3 h-3" /> by meaning
            </span>
          )}
        </span>
        <span className="min-w-0 text-[11px] font-mono text-slate-400 truncate" title={best?.url}>
          {best ? `${pathOf(best.url)} · ${percent(best.score)}` : '—'}
        </span>
        <span className="text-[11px] font-mono text-right">
          {r.cases.total > 0 ? (
            <span className="text-slate-300">
              {r.cases.total} {plural(r.cases.total, 'case', 'cases')}
            </span>
          ) : (
            <span className="text-amber-300">no cases</span>
          )}
        </span>
      </button>

      {open && (
        <div className="pb-3 pl-9 pr-2 space-y-3 text-xs">
          <p className="text-slate-300 whitespace-pre-wrap break-words">{r.text}</p>
          {r.section && <div className="text-[10px] text-content-muted">section: {r.section}</div>}

          <div className="space-y-1.5">
            <div className="text-[10px] font-mono uppercase text-content-muted">
              Pages it points at {r.basis ? `· ${r.basis === 'ai' && r.status !== 'FOUND' ? 'checked by the AI' : (BASIS_LABEL[r.basis] ?? r.basis)}` : ''}
            </div>
            {r.matches.length > 0 ? (
              <ul className="space-y-1.5">
                {r.matches.map((m) => (
                  <MatchDetail key={`${m.url}-${m.basis}`} match={m} />
                ))}
              </ul>
            ) : (
              <p className="text-content-muted italic">{STATUS_META[r.status].hint}</p>
            )}
          </div>

          {r.verification && (
            <div className="p-2 rounded bg-violet-950/20 border border-violet-900/50 space-y-1">
              <div className="flex items-center gap-2 text-[11px]">
                <Bot className="w-3.5 h-3.5 text-violet-300" />
                <span className="font-mono text-violet-200">AI check: {formatStatusLabel(r.verification.verdict)}</span>
                <span className="text-[10px] text-content-muted">{r.verification.by}</span>
              </div>
              {r.verification.reason && <p className="text-slate-300">{r.verification.reason}</p>}
              {r.verification.pages.length > 0 && (
                <div className="text-[10px] text-slate-400 break-all">pages: {r.verification.pages.map(pathOf).join(', ')}</div>
              )}
            </div>
          )}

          <div className="space-y-1">
            <div className="text-[10px] font-mono uppercase text-content-muted">Cases</div>
            {r.cases.total > 0 ? (
              <div className="flex flex-wrap items-center gap-1.5">
                <Chip>{r.cases.total} total</Chip>
                <Chip>{r.cases.approved} approved</Chip>
                <Chip>{r.cases.executed} executed</Chip>
                {techniques.map(([t, n]) => (
                  <Chip key={t} tone="violet">
                    {t} × {n}
                  </Chip>
                ))}
              </div>
            ) : (
              <p className="text-amber-300">No case is tied to this requirement.</p>
            )}
            {r.cases.missing_techniques.length > 0 && (
              <div className="flex flex-wrap items-center gap-1.5">
                <span className="text-[10px] text-content-muted">still without a case:</span>
                {r.cases.missing_techniques.map((t) => (
                  <Chip key={t} tone="amber">
                    {t}
                  </Chip>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </li>
  );
};

// ---------------------------------------------------------------------- the page

type StatusFilter = 'ALL' | GapStatus;

export const RequirementCoveragePage: React.FC = () => {
  const { selectedProjectId, selectedProject } = useProject();
  const navigate = useNavigate();

  // Each of these remembers the project it belongs to, so that for the render in which the selected project changes
  // nothing of the previous project's crawls, choice or report is taken for the new one's.
  const [crawlSet, setCrawlSet] = useState<{ projectId: string; runs: RunListItem[] } | null>(null);
  const crawls = crawlSet && crawlSet.projectId === selectedProjectId ? crawlSet.runs : null;
  const [crawlsLoading, setCrawlsLoading] = useState(false);
  const [crawlsError, setCrawlsError] = useState<ApiError | null>(null);
  const [crawlChoice, setCrawlChoice] = useState<{ projectId: string; id: string | null } | null>(null);
  const crawlId = crawlChoice && crawlChoice.projectId === selectedProjectId ? crawlChoice.id : null;
  const setCrawlId = useCallback(
    (id: string | null) => {
      if (selectedProjectId) setCrawlChoice({ projectId: selectedProjectId, id });
    },
    [selectedProjectId],
  );
  const [role, setRole] = useState('');
  /** Every role of the chosen crawl, remembered from a report that was not limited to one. */
  const [allRoles, setAllRoles] = useState<string[]>([]);

  const [useEmbeddings, setUseEmbeddings] = useState(false);
  const [verify, setVerify] = useState(false);
  const [maxVerifications, setMaxVerifications] = useState(DEFAULT_MAX_VERIFICATIONS);

  const [reportState, setReportState] = useState<{ projectId: string; report: GapCheckResponse } | null>(null);
  const report = reportState && reportState.projectId === selectedProjectId ? reportState.report : null;
  const [running, setRunning] = useState(false);
  const [errorState, setErrorState] = useState<{ projectId: string; error: ApiError } | null>(null);
  const runError = errorState && errorState.projectId === selectedProjectId ? errorState.error : null;
  const runAbortRef = useRef<AbortController | null>(null);
  const crawlsAbortRef = useRef<AbortController | null>(null);

  const [statusFilter, setStatusFilter] = useState<StatusFilter>('ALL');
  const [search, setSearch] = useState('');
  const [untestedOnly, setUntestedOnly] = useState(false);
  const [openRows, setOpenRows] = useState<Set<string>>(new Set());

  // ------------------------------------------------------------------ crawls

  const fetchCrawls = useCallback(async (projectId: string) => {
    crawlsAbortRef.current?.abort();
    const controller = new AbortController();
    crawlsAbortRef.current = controller;
    setCrawlsLoading(true);
    setCrawlsError(null);
    try {
      const runs = await aiqaApi.executions.list(projectId, {}, controller.signal);
      if (controller.signal.aborted) return;
      const found = runs.filter(isCrawlRun);
      setCrawlSet({ projectId, runs: found });
      // The list is newest first. The newest crawl that finished is the one to look at (one still running, or one that
      // failed, left nothing to compare with); when none has, the newest of all.
      const finished = found.find((r) => r.state === 'COMPLETED' || r.state === 'PARTIAL');
      setCrawlChoice({ projectId, id: (finished ?? found[0])?.run_id ?? null });
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      setCrawlsError(err instanceof ApiError ? err : new ApiError('server_error', String(err)));
      setCrawlSet(null);
    } finally {
      if (!controller.signal.aborted) setCrawlsLoading(false);
    }
  }, []);

  useEffect(() => {
    // A project switch: nothing of the previous project's crawls or report applies.
    runAbortRef.current?.abort();
    setRole('');
    setAllRoles([]);
    setRunning(false);
    setOpenRows(new Set());
    setCrawlsError(null);
    if (!selectedProjectId) {
      crawlsAbortRef.current?.abort();
      setCrawlsLoading(false);
      return;
    }
    void fetchCrawls(selectedProjectId);
  }, [selectedProjectId, fetchCrawls]);

  // --------------------------------------------------------------- the check

  const runCheck = useCallback(
    async (projectId: string, body: GapCheckRequest) => {
      runAbortRef.current?.abort();
      const controller = new AbortController();
      runAbortRef.current = controller;
      setRunning(true);
      setErrorState(null);
      try {
        const result = await aiqaApi.gapCheck.run(projectId, body, controller.signal);
        if (controller.signal.aborted) return;
        setReportState({ projectId, report: result });
        setOpenRows(new Set());
        // Only a report that was not limited to one role lists all of them.
        if (!body.role) setAllRoles(result.crawl.roles.map((r) => r.role));
      } catch (err) {
        if (err instanceof DOMException && err.name === 'AbortError') return;
        setErrorState({ projectId, error: err instanceof ApiError ? err : new ApiError('server_error', String(err)) });
        setReportState(null);
      } finally {
        if (!controller.signal.aborted) setRunning(false);
      }
    },
    [],
  );

  const baseBody = useCallback(
    (): GapCheckRequest => ({ ...(crawlId ? { crawl_run_id: crawlId } : {}), ...(role ? { role } : {}) }),
    [crawlId, role],
  );

  // The words check is free: it runs by itself whenever the project, the crawl or the role changes.
  useEffect(() => {
    if (!selectedProjectId || crawlsLoading || crawlsError || !crawls || crawls.length === 0 || !crawlId) return;
    void runCheck(selectedProjectId, baseBody());
  }, [selectedProjectId, crawls, crawlsLoading, crawlsError, crawlId, role, runCheck, baseBody]);

  const handleRunWithOptions = useCallback(() => {
    if (!selectedProjectId || running) return;
    const body: GapCheckRequest = {
      ...baseBody(),
      ...(useEmbeddings ? { use_embeddings: true } : {}),
      ...(verify ? { verify: true, max_verifications: maxVerifications } : {}),
    };
    void runCheck(selectedProjectId, body);
  }, [selectedProjectId, running, baseBody, useEmbeddings, verify, maxVerifications, runCheck]);

  useEffect(
    () => () => {
      runAbortRef.current?.abort();
      crawlsAbortRef.current?.abort();
    },
    [],
  );

  // ----------------------------------------------------------------- filtering

  const requirements = report?.requirements ?? [];
  const counts = useMemo(() => {
    const c: Record<GapStatus, number> = { FOUND: 0, WEAK: 0, MISSING: 0, NOT_CHECKABLE: 0 };
    for (const r of requirements) c[r.status] += 1;
    return c;
  }, [requirements]);

  const visible = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return requirements
      .map((r, index) => ({ r, index }))
      .filter(({ r }) => statusFilter === 'ALL' || r.status === statusFilter)
      .filter(({ r }) => !untestedOnly || r.cases.total === 0)
      .filter(({ r }) => !needle || `${r.key} ${r.text} ${r.section ?? ''}`.toLowerCase().includes(needle));
  }, [requirements, statusFilter, untestedOnly, search]);

  const toggleRow = useCallback((rowId: string) => {
    setOpenRows((prev) => {
      const next = new Set(prev);
      if (next.has(rowId)) next.delete(rowId);
      else next.add(rowId);
      return next;
    });
  }, []);

  // ------------------------------------------------------------------ rendering

  const goToGenerator = () => navigate('/test-generator');

  const errorBody = (err: ApiError, onRetry: () => void): React.ReactNode => {
    const text = err.message;
    if (err.kind === 'validation_error' && /no requirements/i.test(text)) {
      return (
        <EmptyState
          icon={Inbox}
          title="No requirements to check"
          description={text}
          actionLabel="Open Test Case Generator"
          onAction={goToGenerator}
        />
      );
    }
    if (err.kind === 'not_found') {
      return (
        <EmptyState
          icon={Layers}
          title="No crawl to check against"
          description={text}
          actionLabel="Open Test Case Generator"
          onAction={goToGenerator}
          secondaryActionLabel="Try again"
          onSecondaryAction={onRetry}
        />
      );
    }
    return renderResourceErrorState(err, onRetry, {
      forbiddenTitle: 'Not permitted to run this check',
      forbiddenDescription:
        "This key's role or project scope does not permit it. The words check needs to read the project; the embedder and AI options need the Operator role.",
      genericTitle: "Couldn't run the check",
    });
  };

  const summary = report?.summary;
  const roleChoices = allRoles.length > 0 ? allRoles : report ? report.crawl.roles.map((r) => r.role) : [];

  const renderControls = () => (
    <section id="gap-controls" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-3">
      <div className="flex flex-col md:flex-row md:items-end gap-3">
        <div className="space-y-1">
          <label htmlFor="gap-crawl-select" className="block text-[10px] font-mono uppercase text-content-muted">
            Crawl
          </label>
          <select
            id="gap-crawl-select"
            value={crawlId ?? ''}
            onChange={(e) => {
              setCrawlId(e.target.value || null);
              setRole('');
              setAllRoles([]);
            }}
            disabled={!crawls || crawls.length === 0}
            className="rounded-md border border-slate-800 bg-[#020617] hover:bg-slate-900 px-2.5 py-1.5 text-xs text-slate-300 outline-none focus:ring-1 focus:ring-violet-500 font-mono cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed max-w-[320px]"
          >
            {(crawls ?? []).map((c) => (
              <option key={c.run_id} value={c.run_id}>
                {c.name} ({formatStatusLabel(c.state)})
              </option>
            ))}
          </select>
        </div>
        <div className="space-y-1">
          <label htmlFor="gap-role-select" className="block text-[10px] font-mono uppercase text-content-muted">
            Role
          </label>
          <select
            id="gap-role-select"
            value={role}
            onChange={(e) => setRole(e.target.value)}
            disabled={!crawlId}
            className="rounded-md border border-slate-800 bg-[#020617] hover:bg-slate-900 px-2.5 py-1.5 text-xs text-slate-300 outline-none focus:ring-1 focus:ring-violet-500 font-mono cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
          >
            <option value="">All roles</option>
            {roleChoices.map((r) => (
              <option key={r} value={r}>
                {r}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-5 gap-y-2 text-xs text-slate-300">
        <label className="inline-flex items-center gap-2 cursor-pointer">
          <input
            id="gap-use-embeddings"
            type="checkbox"
            checked={useEmbeddings}
            onChange={(e) => setUseEmbeddings(e.target.checked)}
            className="accent-violet-500"
          />
          <span>Also compare meaning (embeddings)</span>
        </label>
        <label className="inline-flex items-center gap-2 cursor-pointer">
          <input
            id="gap-verify"
            type="checkbox"
            checked={verify}
            onChange={(e) => setVerify(e.target.checked)}
            className="accent-violet-500"
          />
          <span>Ask the AI to check the unclear ones</span>
        </label>
        {verify && (
          <label className="inline-flex items-center gap-2">
            <span className="text-slate-400">at most</span>
            <input
              id="gap-max-verifications"
              type="number"
              min={1}
              max={MAX_VERIFICATIONS_LIMIT}
              value={maxVerifications}
              onChange={(e) => {
                const n = Number(e.target.value);
                if (Number.isFinite(n)) setMaxVerifications(Math.min(MAX_VERIFICATIONS_LIMIT, Math.max(1, Math.round(n))));
              }}
              className="w-16 rounded-md border border-slate-800 bg-[#020617] px-2 py-1 text-xs text-slate-200 font-mono outline-none focus:ring-1 focus:ring-violet-500"
            />
            <span className="text-slate-400">requirements</span>
          </label>
        )}
        <button
          type="button"
          id="gap-run-button"
          onClick={handleRunWithOptions}
          disabled={!crawlId || running || (!useEmbeddings && !verify)}
          title={
            !useEmbeddings && !verify
              ? 'The words check runs by itself; tick an option to run one that calls a provider.'
              : 'Runs the check again, this time calling the embedder and/or the AI.'
          }
          className="ml-auto flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-700 hover:bg-violet-600 text-white font-medium text-xs border border-violet-500 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-400"
        >
          {running ? <RotateCcw className="w-3.5 h-3.5 animate-spin" /> : <Play className="w-3.5 h-3.5" />}
          <span>Run with options</span>
        </button>
      </div>
      <p className="text-[10px] text-content-muted">
        The check of words runs by itself and costs nothing. The two options call a provider (the embedder, the AI routed for
        Traceability in Settings) and run only from the button.
      </p>
    </section>
  );

  const tile = (id: string, label: string, value: number, tone: string) => (
    <div id={id} key={id} className="p-2.5 rounded-md bg-[#020617] border border-slate-800">
      <div className={`text-lg font-mono font-semibold ${tone}`}>{value.toLocaleString()}</div>
      <div className="text-[10px] text-content-muted leading-tight">{label}</div>
    </div>
  );

  const renderMethod = () => {
    const method = report?.method;
    if (!method) return null;
    const lines: string[] = [];
    if (method.embeddings.requested) {
      lines.push(
        method.embeddings.used
          ? `Meaning compared with ${method.embeddings.model ?? 'the embedder'}: ${method.embeddings.texts.toLocaleString()} ${plural(method.embeddings.texts, 'text', 'texts')} embedded${method.embeddings.failed > 0 ? `, ${method.embeddings.failed.toLocaleString()} failed` : ''}.`
          : `Meaning was not compared: ${method.embeddings.note ?? 'the embedder could not be used'}.`,
      );
    }
    if (method.verification.requested) {
      const v = method.verification;
      lines.push(
        v.calls > 0
          ? `AI check by ${v.by.join(', ') || 'the router'}: ${v.checked.toLocaleString()} checked in ${v.calls.toLocaleString()} ${plural(v.calls, 'call', 'calls')} — ${v.confirmed.toLocaleString()} confirmed, ${v.not_found.toLocaleString()} not found, ${v.unclear.toLocaleString()} unclear${v.failed > 0 ? `, ${v.failed.toLocaleString()} could not be checked` : ''}.`
          : `The AI did not check anything: ${v.note ?? 'nothing was left to check'}.`,
      );
    }
    if (lines.length === 0) return null;
    return (
      <ul id="gap-method" className="space-y-0.5 text-[11px] text-slate-400">
        {lines.map((l) => (
          <li key={l}>{l}</li>
        ))}
        {[method.embeddings.note, method.verification.note]
          .filter((n): n is string => !!n && !lines.some((l) => l.includes(n)))
          .map((n) => (
            <li key={n} className="text-amber-300">
              {n}
            </li>
          ))}
      </ul>
    );
  };

  const renderReport = () => {
    if (!report || !summary) return null;
    return (
      <>
        <section id="gap-summary" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-3">
          <div className="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-8 gap-2">
            {tile('gap-tile-requirements', 'requirements', summary.requirements, 'text-slate-100')}
            {tile('gap-tile-found', 'found on a page', summary.found, 'text-emerald-300')}
            {tile('gap-tile-weak', 'weak evidence', summary.weak, 'text-amber-300')}
            {tile('gap-tile-missing', 'no UI evidence', summary.missing, 'text-rose-300')}
            {tile('gap-tile-not-checkable', 'not checkable', summary.not_checkable, 'text-slate-400')}
            {tile('gap-tile-pages', 'pages crawled', summary.pages, 'text-slate-100')}
            {tile('gap-tile-undocumented', 'pages not in the documents', summary.undocumented_pages, 'text-amber-300')}
            {tile('gap-tile-cases', 'cases tied to a requirement', summary.cases, 'text-violet-300')}
          </div>
          <div className="flex flex-wrap gap-x-5 gap-y-1 text-[11px] text-slate-400">
            <span>
              <strong className="text-slate-200">{summary.with_cases.toLocaleString()}</strong> of{' '}
              {summary.requirements.toLocaleString()} {plural(summary.requirements, 'requirement has', 'requirements have')} a case
            </span>
            <span id="gap-shown-untested" className={summary.shown_without_cases > 0 ? 'text-amber-300' : ''}>
              <strong>{summary.shown_without_cases.toLocaleString()}</strong> shown by the application, no case tests{' '}
              {plural(summary.shown_without_cases, 'it', 'them')}
            </span>
            <span id="gap-tested-unshown" className={summary.tested_without_evidence > 0 ? 'text-rose-300' : ''}>
              <strong>{summary.tested_without_evidence.toLocaleString()}</strong> with cases, but no sign of{' '}
              {plural(summary.tested_without_evidence, 'it', 'them')} in the crawl
            </span>
          </div>
          <p className="text-[10px] text-content-muted">
            &ldquo;Found&rdquo; is evidence that a feature has somewhere to live — a page carries the words the requirement is
            about — not proof that it works. &ldquo;No UI evidence&rdquo; means no page the crawl reached carries them: the
            crawl follows links, so a page that opens only from a button or a form is not among them. Open a row to see the
            words and the elements behind it.
          </p>
          {report.notes.length > 0 && (
            <ul id="gap-notes" className="space-y-1">
              {report.notes.map((n) => (
                <li key={n} className="flex items-start gap-1.5 p-2 rounded bg-amber-950/30 border border-amber-800/60 text-[11px] text-amber-300">
                  <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
                  <span>{n}</span>
                </li>
              ))}
            </ul>
          )}
          {renderMethod()}
        </section>

        <section id="gap-requirements" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-3">
          <div className="flex flex-wrap items-center gap-2">
            <div role="group" aria-label="Filter by status" className="flex flex-wrap items-center gap-1.5">
              <button
                type="button"
                id="gap-filter-ALL"
                aria-pressed={statusFilter === 'ALL'}
                onClick={() => setStatusFilter('ALL')}
                className={`px-2 py-1 rounded border text-[11px] font-mono cursor-pointer ${statusFilter === 'ALL' ? 'bg-violet-950/60 border-violet-600 text-violet-200' : 'bg-[#020617] border-slate-800 text-slate-300 hover:bg-slate-900'}`}
              >
                All ({requirements.length})
              </button>
              {STATUS_ORDER.map((s) => (
                <button
                  key={s}
                  type="button"
                  id={`gap-filter-${s}`}
                  aria-pressed={statusFilter === s}
                  onClick={() => setStatusFilter(s)}
                  className={`px-2 py-1 rounded border text-[11px] font-mono cursor-pointer ${statusFilter === s ? 'bg-violet-950/60 border-violet-600 text-violet-200' : 'bg-[#020617] border-slate-800 text-slate-300 hover:bg-slate-900'}`}
                >
                  {STATUS_META[s].label} ({counts[s]})
                </button>
              ))}
            </div>
            <label className="inline-flex items-center gap-1.5 text-[11px] text-slate-300 cursor-pointer">
              <input
                id="gap-untested-only"
                type="checkbox"
                checked={untestedOnly}
                onChange={(e) => setUntestedOnly(e.target.checked)}
                className="accent-violet-500"
              />
              only those with no case
            </label>
            <div className="relative ml-auto">
              <Search className="w-3.5 h-3.5 text-content-muted absolute left-2 top-1/2 -translate-y-1/2" />
              <input
                id="gap-search"
                type="search"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search requirements"
                aria-label="Search requirements"
                className="w-56 rounded-md border border-slate-800 bg-[#020617] pl-7 pr-2 py-1.5 text-xs text-slate-200 outline-none focus:ring-1 focus:ring-violet-500"
              />
            </div>
          </div>

          {visible.length === 0 ? (
            <p id="gap-no-rows" className="text-xs text-content-muted italic py-4 text-center">
              No requirement matches this filter.
            </p>
          ) : (
            <ul id="gap-requirement-list">
              {visible.map(({ r, index }) => {
                const rowId = `gap-requirement-${index}`;
                return (
                  <RequirementRow
                    key={rowId}
                    id={rowId}
                    requirement={r}
                    open={openRows.has(rowId)}
                    onToggle={() => toggleRow(rowId)}
                  />
                );
              })}
            </ul>
          )}
        </section>

        <section id="gap-undocumented-pages" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-2">
          <h2 className="text-xs font-semibold text-slate-100">Pages not in the documents</h2>
          {report.undocumented_pages.length === 0 ? (
            <p className="flex items-center gap-1.5 text-xs text-emerald-300">
              <CheckCircle2 className="w-3.5 h-3.5" /> Every crawled page is pointed at by at least one requirement.
            </p>
          ) : (
            <>
              <p className="text-[11px] text-slate-400">
                The crawl reached {report.undocumented_pages.length.toLocaleString()}{' '}
                {plural(report.undocumented_pages.length, 'page', 'pages')} that no requirement mentions: a feature the
                documents do not describe, or a page the requirements word differently.
              </p>
              <ul className="space-y-1.5">
                {report.undocumented_pages.map((p) => (
                  <li key={p.url} className="p-2 rounded bg-[#020617] border border-slate-800 space-y-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="font-mono text-[11px] text-slate-100 break-all">{pathOf(p.url)}</span>
                      {p.title && <span className="text-[11px] text-slate-400">{p.title}</span>}
                      <span className="text-[10px] text-content-muted">
                        {p.element_count.toLocaleString()} {plural(p.element_count, 'element', 'elements')}
                        {p.roles.length > 0 ? ` · reached by ${p.roles.join(', ')}` : ''}
                      </span>
                    </div>
                    {p.elements.length > 0 && (
                      <div className="flex flex-wrap gap-1">
                        {p.elements.map((e) => (
                          <Chip key={e}>{e}</Chip>
                        ))}
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            </>
          )}
        </section>
      </>
    );
  };

  const renderBody = () => {
    if (!selectedProjectId) {
      return (
        <EmptyState
          icon={FolderOpen}
          title="No project selected"
          description="Choose a project from the selector in the header to check its requirements against the application."
        />
      );
    }
    if (crawlsLoading) {
      return (
        <StateCard
          icon={Layers}
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
          title="Loading crawls"
          description={`Fetching the executions of ${selectedProject?.name ?? 'this project'}…`}
        />
      );
    }
    if (crawlsError) {
      return renderResourceErrorState(crawlsError, () => selectedProjectId && void fetchCrawls(selectedProjectId), {
        forbiddenTitle: 'Not permitted to view this project',
        forbiddenDescription: "This key's project scope does not permit viewing this project's executions.",
      });
    }
    if (crawls && crawls.length === 0) {
      return (
        <EmptyState
          icon={Layers}
          title="No crawl yet"
          description={`${selectedProject?.name ?? 'This project'} has not crawled its application, so there are no pages to compare the requirements with. Crawl it from the Test Case Generator (Crawl This Application).`}
          actionLabel="Open Test Case Generator"
          onAction={goToGenerator}
        />
      );
    }
    return (
      <div className="space-y-4">
        {renderControls()}
        {running && !report && (
          <StateCard
            icon={ListChecks}
            iconClassName="text-violet-400"
            spin
            statusLabel="CHECKING…"
            statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
            title="Lining the requirements up with the crawl"
            description="Reading the requirements, the crawl's pages and the saved cases…"
          />
        )}
        {runError && (
          <div id="gap-error">
            {errorBody(runError, () => selectedProjectId && void runCheck(selectedProjectId, baseBody()))}
          </div>
        )}
        {report && (
          <div className={running ? 'opacity-60 pointer-events-none' : ''} aria-busy={running}>
            <div className="space-y-4">{renderReport()}</div>
          </div>
        )}
      </div>
    );
  };

  return (
    <div id="aiqa-requirement-coverage-page" className="space-y-4 animate-in fade-in duration-200 text-slate-200">
      <header
        id="gap-header"
        className="flex flex-col md:flex-row md:items-center justify-between gap-3 p-4 rounded-lg bg-[#0B1120] border border-slate-800/90 shadow-sm"
      >
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <span className="p-1.5 rounded bg-violet-950/80 border border-violet-700/60 text-violet-400">
              <ListChecks className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-slate-100 font-sans tracking-tight">Requirement Coverage</h1>
            <span className="px-2 py-0.5 rounded bg-violet-950/60 border border-violet-800/60 text-[10px] font-mono text-violet-300 uppercase">
              GAP CHECK
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans">
            {selectedProject
              ? `What the documents of ${selectedProject.name} say, against what its application shows`
              : 'What the documents say, against what the application shows'}
          </p>
        </div>
      </header>

      <div aria-live="polite">{renderBody()}</div>
    </div>
  );
};
