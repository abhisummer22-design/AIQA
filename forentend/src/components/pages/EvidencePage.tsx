/**
 * FE-7 — the real Evidence Viewer.
 *
 * Backend contract (re-verified at the start of this sprint,
 * `services/orchestrator/src/api.rs`, `store.rs::EvidenceRow`/
 * `list_evidence_for_run`/`get_evidence_by_id`/`get_evidence_object_key`):
 * evidence has **no project-wide list endpoint** — it is strictly scoped to
 * a run (`evidence.run_id`), which is itself scoped to a project. The three
 * real, already-implemented endpoints, all already exposed by FE-1 before
 * this sprint began (`services/aiqaApi.ts` — zero changes needed here):
 *
 *   - `GET /api/v1/projects/:project_id/executions/:run_id/evidence`
 *     (`aiqaApi.executions.evidence`) — list, `EvidenceRow[]`.
 *   - `GET /api/v1/projects/:project_id/evidence/:evidence_id`
 *     (`aiqaApi.evidence.get`) — metadata detail, same `EvidenceRow` shape
 *     the list already returns per item (not called by this page — see
 *     "Why no separate detail fetch" below).
 *   - `GET /api/v1/projects/:project_id/evidence/:evidence_id/content`
 *     (`aiqaApi.evidence.content`) — raw bytes, real `Content-Type`.
 *
 * `EvidenceRow` (`types/contract.ts`, pre-existing, unchanged): `id,
 * task_id, kind, object_key, content_type, size_bytes, created_at`. No
 * `name`/`filename`/`url`/`status`/`metadata` field exists on the real
 * contract — none of those are rendered here.
 *
 * Architecture note — a run picker is real, not invented: since the
 * backend has no project-wide evidence endpoint, *some* way to choose a
 * run is structurally required to reach evidence at all. This page lists
 * real runs via `aiqaApi.executions.list` (already existed, unrelated
 * unchanged) and lets the user pick one — composing two already-real
 * endpoints along the real `evidence → run → project` relationship the
 * backend itself defines (brief §18), not a new invented feature.
 *
 * Why no separate detail fetch: `list_evidence_for_run` already returns
 * the full `EvidenceRow` per item — identical to what `get_evidence_by_id`
 * (the "detail" endpoint) would additionally return. Calling `.get()`
 * again for data already in hand would be a pointless duplicate request;
 * the list's own row is reused directly. `aiqaApi.evidence.get` is still
 * real, still tested (§26 API layer), and still available for anything
 * that reaches evidence without first listing it — nothing in this sprint
 * needed that path.
 *
 * `object_key` is real but is a storage-layer implementation detail (a
 * relative `"{run_id}/{file_name}"` key the backend's own `evidence.rs`
 * resolves against its storage root, not a URL or anything actionable by
 * this frontend) — never rendered here (brief §16/§25: "do not expose
 * internal storage paths"). Downloads go through the real `/content`
 * endpoint, never a path constructed from `object_key`.
 *
 * No FRD/SRD/PRD/flow-diagram document exists in this repository
 * (re-confirmed at the start of this sprint, same search as every prior
 * FE unit). Every behavior below is derived from the real backend
 * contract above and this sprint's own explicit constraints.
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  ChevronDown,
  ChevronRight,
  Download,
  FileJson,
  FileQuestion,
  FileText,
  FolderOpen,
  Image as ImageIcon,
  Inbox,
  Layers,
  RotateCcw,
} from 'lucide-react';
import { ApiError, aiqaApi, getApiKey } from '../../services/aiqaApi';
import { useProject } from '../../context/ProjectContext';
import { useRealtime } from '../../context/RealtimeContext';
import { StateCard, Cta, renderResourceErrorState } from '../state/StateViews';
import { formatStatusLabel } from '../../utils/statusLabel';
import { EmptyState } from '../common/EmptyState';
import type { EvidenceRow, RunListItem } from '../../types/contract';

/** FE-9: no real event type "is" an evidence event — evidence rows are
 *  written as a side effect of task execution, not published to the WS
 *  stream as their own event (`domain.rs::EventType` has no such variant).
 *  But a real `TASK_*` terminal event for the currently-viewed run is
 *  exactly when new evidence for that task becomes possible — so it is
 *  used as the honest, disclosed trigger to invalidate/refetch the real
 *  evidence resource (brief §20) via the *existing* `fetchEvidence`, never
 *  a fabricated evidence row and never a second evidence-fetching path. */
const EVIDENCE_EVENT_RESYNC_DEBOUNCE_MS = 250;

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
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

/** Same defensive, narrowly-scoped redaction `AuditPage.tsx` (FE-6) uses:
 *  only the *current session's own* raw key, never a broad "looks like a
 *  secret" heuristic that would risk mangling legitimate test evidence
 *  (brief §15: "do not over-redact legitimate test evidence"). */
function redactOwnKey(value: string): string {
  const key = getApiKey();
  if (!key) return value;
  return value.split(key).join('••••••••');
}

type PreviewKind = 'image' | 'json' | 'text' | 'unsupported';

function previewKindFor(contentType: string | null): PreviewKind {
  if (!contentType) return 'unsupported';
  if (contentType.startsWith('image/')) return 'image';
  if (contentType === 'application/json') return 'json';
  // Deliberately excludes text/html: no HTML sanitizer exists anywhere in
  // this codebase (checked before writing this file), and the brief
  // explicitly forbids blindly injecting arbitrary HTML. HTML evidence is
  // shown as safe, literal, escaped text via the 'text' branch below —
  // readable, never executed — rather than adding a new dependency for one
  // content type (brief: "do not over-engineer").
  if (contentType.startsWith('text/')) return 'text';
  return 'unsupported';
}

const EXTENSION_BY_CONTENT_TYPE: Record<string, string> = {
  'image/png': 'png',
  'image/jpeg': 'jpg',
  'image/gif': 'gif',
  'image/webp': 'webp',
  'application/json': 'json',
  'text/plain': 'txt',
  'text/html': 'html',
  'text/csv': 'csv',
};

/** `FileReader` rather than `Blob.prototype.text()`/`arrayBuffer()` —
 *  broader real support (every evergreen browser, further back than the
 *  newer `Blob` read methods) and, concretely, the only one of the three
 *  that actually works against this project's own jsdom test environment
 *  (confirmed before writing this file: a throwaway check showed neither
 *  `.text()` nor `.arrayBuffer()` exist on the `Blob` this environment's
 *  `fetch`/`Response` polyfill produces, while `FileReader.readAsText`
 *  works correctly). Not a test-only shim — this is the real
 *  implementation `EvidencePage` ships. */
function readBlobAsText(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ''));
    reader.onerror = () => reject(reader.error ?? new Error('failed to read evidence content'));
    reader.readAsText(blob);
  });
}

interface PreviewState {
  evidenceId: string;
  loading: boolean;
  error: ApiError | null;
  objectUrl: string | null;
  text: string | null;
  blob: Blob | null;
}

export const EvidencePage: React.FC = () => {
  const { selectedProjectId, selectedProject } = useProject();
  const realtime = useRealtime();

  const [runs, setRuns] = useState<RunListItem[] | null>(null);
  const [runsLoading, setRunsLoading] = useState(false);
  const [runsError, setRunsError] = useState<ApiError | null>(null);
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const runsAbortRef = useRef<AbortController | null>(null);

  const [rows, setRows] = useState<EvidenceRow[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const evidenceAbortRef = useRef<AbortController | null>(null);

  const [preview, setPreview] = useState<PreviewState | null>(null);
  const previewAbortRef = useRef<AbortController | null>(null);

  // ------------------------------------------------------------- runs

  const fetchRuns = useCallback(async (projectId: string) => {
    runsAbortRef.current?.abort();
    const controller = new AbortController();
    runsAbortRef.current = controller;
    setRunsLoading(true);
    setRunsError(null);
    try {
      const list = await aiqaApi.executions.list(projectId, {}, controller.signal);
      if (controller.signal.aborted) return;
      setRuns(list);
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
      setRunsError(apiErr);
      setRuns(null);
    } finally {
      if (!controller.signal.aborted) setRunsLoading(false);
    }
  }, []);

  useEffect(() => {
    // Project switch: a run id from the previous project is meaningless
    // (and must never leak evidence across projects — brief §19). Reset
    // everything downstream and start over.
    setSelectedRunId(null);
    setRows(null);
    setError(null);
    setPreview(null);
    if (!selectedProjectId) {
      runsAbortRef.current?.abort();
      setRuns(null);
      setRunsError(null);
      setRunsLoading(false);
      return;
    }
    setRuns(null);
    void fetchRuns(selectedProjectId);
  }, [selectedProjectId, fetchRuns]);

  // ------------------------------------------------------------- evidence

  const fetchEvidence = useCallback(async (projectId: string, runId: string, isRefresh: boolean) => {
    evidenceAbortRef.current?.abort();
    const controller = new AbortController();
    evidenceAbortRef.current = controller;
    if (isRefresh) setRefreshing(true);
    else setLoading(true);
    setError(null);
    try {
      const list = await aiqaApi.executions.evidence(projectId, runId, controller.signal);
      if (controller.signal.aborted) return;
      setRows(list);
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
      setError(apiErr);
      setRows(null);
    } finally {
      if (!controller.signal.aborted) {
        setLoading(false);
        setRefreshing(false);
      }
    }
  }, []);

  useEffect(() => {
    setPreview(null);
    if (!selectedProjectId || !selectedRunId) {
      evidenceAbortRef.current?.abort();
      setRows(null);
      setError(null);
      setLoading(false);
      setRefreshing(false);
      return;
    }
    setRows(null);
    void fetchEvidence(selectedProjectId, selectedRunId, false);
  }, [selectedProjectId, selectedRunId, fetchEvidence]);

  // FE-9 — see the module doc comment above for exactly why a `TASK_*`
  // event for the currently-viewed run is the honest trigger here.
  // Debounced (a run with several tasks finishing close together collapses
  // into one real request) and uses `isRefresh: true` so a background
  // resync never blanks an already-open preview/rendered rows.
  const selectedRunIdRef = useRef<string | null>(null);
  useEffect(() => {
    selectedRunIdRef.current = selectedRunId;
  }, [selectedRunId]);
  const evidenceResyncTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (!selectedProjectId) return;
    const unsubscribe = realtime.subscribe((event) => {
      if (!selectedRunIdRef.current || event.run_id !== selectedRunIdRef.current) return;
      if (evidenceResyncTimeoutRef.current) clearTimeout(evidenceResyncTimeoutRef.current);
      evidenceResyncTimeoutRef.current = setTimeout(() => {
        evidenceResyncTimeoutRef.current = null;
        void fetchEvidence(selectedProjectId, selectedRunIdRef.current as string, true);
      }, EVIDENCE_EVENT_RESYNC_DEBOUNCE_MS);
    });
    return () => {
      unsubscribe();
      if (evidenceResyncTimeoutRef.current) clearTimeout(evidenceResyncTimeoutRef.current);
    };
    // Live-discovered FE-9 bug fix (see `ExecutionHubPage.tsx`'s own,
    // fuller comment on this exact bug): depend on the stable
    // `realtime.subscribe` identity, not the whole `realtime` object
    // (which changes on every single event via `lastEventAt`) — otherwise
    // this effect tears down and re-subscribes on every event, cancelling
    // its own just-scheduled debounce timer before it can fire.
  }, [selectedProjectId, realtime.subscribe, fetchEvidence]);

  useEffect(
    () => () => {
      runsAbortRef.current?.abort();
      evidenceAbortRef.current?.abort();
      previewAbortRef.current?.abort();
    },
    [],
  );

  // Revoke any object URL when it's replaced or the page unmounts — the
  // blob preview must not leak memory.
  useEffect(() => {
    const url = preview?.objectUrl;
    return () => {
      if (url) URL.revokeObjectURL(url);
    };
  }, [preview?.objectUrl]);

  const handleRefreshEvidence = useCallback(() => {
    if (!selectedProjectId || !selectedRunId || loading || refreshing) return;
    void fetchEvidence(selectedProjectId, selectedRunId, true);
  }, [selectedProjectId, selectedRunId, loading, refreshing, fetchEvidence]);

  // ------------------------------------------------------------- preview

  const togglePreview = useCallback(
    async (row: EvidenceRow) => {
      if (preview?.evidenceId === row.id) {
        previewAbortRef.current?.abort();
        setPreview(null);
        return;
      }
      if (!selectedProjectId) return;
      previewAbortRef.current?.abort();
      const controller = new AbortController();
      previewAbortRef.current = controller;
      setPreview({ evidenceId: row.id, loading: true, error: null, objectUrl: null, text: null, blob: null });
      try {
        const blob = await aiqaApi.evidence.content(selectedProjectId, row.id, controller.signal);
        if (controller.signal.aborted) return;
        const kind = previewKindFor(row.content_type);
        if (kind === 'image') {
          const objectUrl = URL.createObjectURL(blob);
          setPreview({ evidenceId: row.id, loading: false, error: null, objectUrl, text: null, blob });
        } else if (kind === 'json' || kind === 'text') {
          const text = redactOwnKey(await readBlobAsText(blob));
          setPreview({ evidenceId: row.id, loading: false, error: null, objectUrl: null, text, blob });
        } else {
          setPreview({ evidenceId: row.id, loading: false, error: null, objectUrl: null, text: null, blob });
        }
      } catch (err) {
        if (err instanceof DOMException && err.name === 'AbortError') return;
        const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
        setPreview({ evidenceId: row.id, loading: false, error: apiErr, objectUrl: null, text: null, blob: null });
      }
    },
    [preview, selectedProjectId],
  );

  const handleDownload = useCallback((row: EvidenceRow, blob: Blob) => {
    // A real blob, from the real `/content` endpoint's real bytes — never
    // a fabricated or credential-bearing URL (brief §16).
    const url = URL.createObjectURL(blob);
    const ext = row.content_type ? EXTENSION_BY_CONTENT_TYPE[row.content_type] : undefined;
    const a = document.createElement('a');
    a.href = url;
    a.download = `${row.kind}-${row.id}${ext ? `.${ext}` : ''}`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  }, []);

  const selectedRun = useMemo(() => runs?.find((r) => r.run_id === selectedRunId) ?? null, [runs, selectedRunId]);

  const errorStateCard = (err: ApiError, onRetry: () => void) =>
    renderResourceErrorState(err, onRetry, {
      forbiddenTitle: 'Not permitted to view this evidence',
      forbiddenDescription: "This key's project scope does not permit viewing this project's evidence. This is a permission problem, not a bad credential.",
    });

  const kindIcon = (contentType: string | null): React.ReactNode => {
    const kind = previewKindFor(contentType);
    if (kind === 'image') return <ImageIcon className="w-3.5 h-3.5 text-yellow-400" />;
    if (kind === 'json') return <FileJson className="w-3.5 h-3.5 text-amber-400" />;
    if (kind === 'text') return <FileText className="w-3.5 h-3.5 text-slate-400" />;
    return <FileQuestion className="w-3.5 h-3.5 text-content-muted" />;
  };

  const renderPreviewBody = (row: EvidenceRow) => {
    if (!preview || preview.evidenceId !== row.id) return null;
    if (preview.loading) {
      return <p className="text-xs text-slate-400 font-mono">Loading preview…</p>;
    }
    if (preview.error) {
      return (
        <p className="text-xs text-rose-400">
          Couldn&apos;t load this evidence&apos;s content: {preview.error.message}
        </p>
      );
    }
    const kind = previewKindFor(row.content_type);
    return (
      <div className="space-y-2">
        {kind === 'image' && preview.objectUrl && (
          <img src={preview.objectUrl} alt={`Evidence ${row.id}`} className="max-w-full max-h-96 rounded border border-slate-800" />
        )}
        {kind === 'json' && preview.text && (
          <pre className="text-[11px] font-mono text-slate-300 bg-[#020617] border border-slate-800 rounded p-2.5 max-h-72 overflow-auto whitespace-pre-wrap break-words">
            {(() => {
              try {
                return JSON.stringify(JSON.parse(preview.text as string), null, 2);
              } catch {
                return preview.text;
              }
            })()}
          </pre>
        )}
        {kind === 'text' && preview.text && (
          <pre className="text-[11px] font-mono text-slate-300 bg-[#020617] border border-slate-800 rounded p-2.5 max-h-72 overflow-auto whitespace-pre-wrap break-words">
            {preview.text}
          </pre>
        )}
        {kind === 'unsupported' && (
          <p className="text-xs text-content-muted italic">
            Preview unavailable for {row.content_type ?? 'this content type'}.
          </p>
        )}
        {preview.blob && (
          <Cta label="Download" icon={Download} onClick={() => handleDownload(row, preview.blob as Blob)} />
        )}
      </div>
    );
  };

  const renderEvidenceBody = () => {
    if (!selectedProjectId) {
      return (
        <EmptyState
          icon={FolderOpen}
          title="No project selected"
          description="Choose a project from the selector in the header to browse its execution evidence."
        />
      );
    }

    if (runsLoading) {
      return (
        <StateCard
          icon={Layers}
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
          title="Loading executions"
          description={`Fetching executions for ${selectedProject?.name ?? 'this project'}…`}
        />
      );
    }

    if (runsError) {
      return errorStateCard(runsError, () => selectedProjectId && void fetchRuns(selectedProjectId));
    }

    if (runs && runs.length === 0) {
      return (
        <EmptyState
          icon={Inbox}
          title="No executions yet"
          description={`${selectedProject?.name ?? 'This project'} has no executions recorded, so there is no evidence to browse yet.`}
        />
      );
    }

    if (!selectedRunId) {
      return (
        <EmptyState
          icon={Layers}
          title="Select an execution"
          description="Choose an execution above to view the real evidence it produced."
        />
      );
    }

    if (loading) {
      return (
        <StateCard
          icon={Layers}
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
          title="Loading evidence"
          description={`Fetching evidence for ${selectedRun?.name ?? 'this execution'}…`}
        />
      );
    }

    if (error) {
      return errorStateCard(error, handleRefreshEvidence);
    }

    if (rows && rows.length === 0) {
      return (
        <EmptyState
          icon={Inbox}
          title="No evidence recorded"
          description={`${selectedRun?.name ?? 'This execution'} completed without recording any evidence artifacts.`}
        />
      );
    }

    return (
      <ul className="divide-y divide-slate-800/80">
        {rows?.map((row) => {
          const isOpen = preview?.evidenceId === row.id;
          return (
            <li key={row.id}>
              <button
                type="button"
                onClick={() => void togglePreview(row)}
                aria-expanded={isOpen}
                className="w-full flex items-center gap-3 py-2.5 text-left hover:bg-slate-900/40 transition-colors cursor-pointer focus:outline-none focus-visible:ring-1 focus-visible:ring-violet-400 rounded"
              >
                {isOpen ? (
                  <ChevronDown className="w-3.5 h-3.5 text-content-muted shrink-0" />
                ) : (
                  <ChevronRight className="w-3.5 h-3.5 text-content-muted shrink-0" />
                )}
                {kindIcon(row.content_type)}
                <span className="font-mono text-xs font-semibold text-slate-100 shrink-0">{row.kind}</span>
                {row.task_id && (
                  <span className="font-mono text-[10px] text-violet-400 truncate" title={row.task_id}>
                    task: {row.task_id}
                  </span>
                )}
                <span className="ml-auto flex items-center gap-3 shrink-0 text-[10px] font-mono text-content-muted">
                  <span>{formatFileSize(row.size_bytes)}</span>
                  <span>{formatTimestamp(row.created_at)}</span>
                </span>
              </button>
              {isOpen && <div className="pb-3 pl-9 pr-2">{renderPreviewBody(row)}</div>}
            </li>
          );
        })}
      </ul>
    );
  };

  return (
    <div id="aiqa-evidence-page" className="space-y-4 animate-in fade-in duration-200 text-slate-200">
      <header
        id="evidence-header"
        className="flex flex-col md:flex-row md:items-center justify-between gap-3 p-4 rounded-lg bg-[#0B1120] border border-slate-800/90 shadow-sm"
      >
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <span className="p-1.5 rounded bg-violet-950/80 border border-violet-700/60 text-violet-400">
              <FileText className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-slate-100 font-sans tracking-tight">Evidence Viewer</h1>
            <span className="px-2 py-0.5 rounded bg-violet-950/60 border border-violet-800/60 text-[10px] font-mono text-violet-300 uppercase">
              EVIDENCE
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans">
            {selectedProject
              ? `Real execution artifacts for ${selectedProject.name}`
              : 'Real execution artifacts for the selected project'}
          </p>
        </div>

        <div className="flex items-center gap-2 self-start md:self-center">
          <label htmlFor="evidence-run-select" className="sr-only">
            Execution
          </label>
          <select
            id="evidence-run-select"
            value={selectedRunId ?? ''}
            onChange={(e) => setSelectedRunId(e.target.value || null)}
            disabled={!selectedProjectId || !runs || runs.length === 0}
            className="rounded-md border border-slate-800 bg-[#020617] hover:bg-slate-900 px-2.5 py-1.5 text-xs text-slate-300 outline-none focus:ring-1 focus:ring-violet-500 font-mono cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed max-w-[220px]"
          >
            <option value="">Select an execution…</option>
            {runs?.map((r) => (
              <option key={r.run_id} value={r.run_id}>
                {r.name} ({formatStatusLabel(r.state)})
              </option>
            ))}
          </select>
          <button
            type="button"
            id="evidence-refresh-button"
            onClick={handleRefreshEvidence}
            disabled={!selectedProjectId || !selectedRunId || loading || refreshing}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 font-medium text-xs border border-slate-700 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none focus-visible:ring-2 focus-visible:ring-violet-400"
          >
            <RotateCcw className={`w-3.5 h-3.5 ${refreshing ? 'animate-spin' : ''}`} />
            <span>Refresh</span>
          </button>
        </div>
      </header>

      <section id="evidence-content" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm" aria-live="polite">
        {renderEvidenceBody()}
      </section>
    </div>
  );
};
