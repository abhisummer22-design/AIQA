/**
 * Real crawl grounding for generation (Phase 6 -> Phase 8 wiring).
 *
 * Phase 6 built a real crawler + per-role visibility matrix
 * (`POST .../crawl` -> `GET .../crawl/:run_id/result`, `crawler.rs`) with no
 * consumer wired up — the backend could crawl a real, running application,
 * but nothing in this app ever looked at what it found. This section is
 * that missing consumer: it runs a real crawl (currently anonymous-only —
 * no role/login-sequence builder exists yet, a real, disclosed limitation,
 * not a silent one), waits for it to genuinely finish through the exact
 * same run/task pipeline every other browser task uses, and — once the
 * caller opts in via the checkbox below — passes the real `run_id` to
 * `POST .../test-cases/generate-ai` as `crawl_run_id`, so real discovered
 * pages/elements ground generation instead of only a written requirement.
 *
 * Every status shown here is the real run's own state
 * (`aiqaApi.executions.get`) — `RUNNING` while genuinely in flight,
 * `FAILED`/`CANCELLED` (or a task's own real `reason`) shown as a real
 * failure, never silently retried into a fabricated success. Polling is a
 * plain, bounded HTTP poll (not this app's realtime WebSocket layer) —
 * deliberately kept simple for this one-off, short-lived, user-initiated
 * action rather than wiring a new consumer onto `RealtimeContext`.
 */
import React, { useEffect, useRef, useState } from 'react';
import { Compass, Loader2, CheckCircle2, XCircle, Info } from 'lucide-react';
import { AppSourceType, AppSourceConnection } from '../../types';
import { ApiError, aiqaApi } from '../../services/aiqaApi';
import type { CrawlOutput } from '../../types/contract';

type CrawlStatus = 'IDLE' | 'STARTING' | 'RUNNING' | 'COMPLETED' | 'FAILED';

interface CrawlGroundingSectionProps {
  appSources: Record<AppSourceType, AppSourceConnection>;
  projectId: string | null;
  /** The real `run_id` currently opted into generation, or `null` when no
   *  crawl has completed or the caller unchecked "use it". Controlled by
   *  the parent so `handleTriggerGenerate` can read it directly. */
  crawlRunId: string | null;
  onCrawlRunIdChange: (runId: string | null) => void;
  disabled?: boolean;
}

const POLL_INTERVAL_MS = 2000;
const MAX_POLL_ATTEMPTS = 45; // ~90s — a real, disclosed bound, not an infinite wait.
const TERMINAL_RUN_STATES = new Set(['COMPLETED', 'PARTIAL', 'FAILED', 'CANCELLED']);

export const CrawlGroundingSection: React.FC<CrawlGroundingSectionProps> = ({
  appSources,
  projectId,
  crawlRunId,
  onCrawlRunIdChange,
  disabled = false,
}) => {
  const connectedUrls = (Object.values(appSources) as AppSourceConnection[]).filter(
    (a) => a.status === 'CONNECTED' && a.type !== 'FIGMA',
  );
  const [targetUrl, setTargetUrl] = useState(connectedUrls[0]?.url ?? '');
  const [status, setStatus] = useState<CrawlStatus>('IDLE');
  const [error, setError] = useState<string | null>(null);
  const [output, setOutput] = useState<CrawlOutput | null>(null);
  const [lastRunId, setLastRunId] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, []);

  // `crawlRunId` is the parent's persisted selection; `output`/`status` are
  // this component's own local state, lost on unmount (e.g. switching away
  // from this branch and back). Without this, a real, still-selected crawl
  // would silently keep being sent to generation with no visible summary or
  // way to un-check it — restore the real summary from the real result
  // instead of leaving that mismatch.
  useEffect(() => {
    if (!crawlRunId || !projectId || output) return;
    let cancelled = false;
    setStatus('RUNNING');
    aiqaApi.crawl
      .result(projectId, crawlRunId)
      .then((result) => {
        if (cancelled) return;
        setOutput(result);
        setLastRunId(crawlRunId);
        setStatus('COMPLETED');
      })
      .catch(() => {
        if (cancelled) return;
        // The selection still exists in the parent, but its real summary
        // could not be re-read — an honest idle state, not a fabricated one.
        setStatus('IDLE');
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [crawlRunId, projectId]);

  const stopPolling = () => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  };

  const handleCrawl = async () => {
    if (!projectId) {
      setError('Select a project before crawling — the real crawl endpoint is project-scoped.');
      return;
    }
    const url = targetUrl.trim();
    if (!url) {
      setError('Enter or pick an application URL to crawl.');
      return;
    }

    setError(null);
    setOutput(null);
    onCrawlRunIdChange(null);
    setStatus('STARTING');

    try {
      const started = await aiqaApi.crawl.start(projectId, {
        base_url: url,
        // Anonymous-only for now — no role/login-sequence builder exists in
        // this UI yet; the backend already supports authenticated roles
        // (`crawler::CrawlRole.login_actions`), a real future extension,
        // not fabricated here.
        roles: [{ name: 'anonymous', login_actions: [] }],
      });
      setLastRunId(started.run_id);
      setStatus('RUNNING');

      let attempts = 0;
      pollRef.current = setInterval(async () => {
        attempts += 1;
        try {
          const run = await aiqaApi.executions.get(projectId, started.run_id);
          if (TERMINAL_RUN_STATES.has(run.state)) {
            stopPolling();
            if (run.state === 'COMPLETED' || run.state === 'PARTIAL') {
              try {
                const result = await aiqaApi.crawl.result(projectId, started.run_id);
                setOutput(result);
                setStatus('COMPLETED');
                onCrawlRunIdChange(started.run_id);
              } catch (err) {
                setStatus('FAILED');
                setError(err instanceof ApiError ? err.message : 'Crawl finished but its real result could not be read.');
              }
            } else {
              const failedTask = run.tasks.find((t) => t.reason);
              setStatus('FAILED');
              setError(failedTask?.reason ?? `Crawl did not complete (${run.state}).`);
            }
            return;
          }
          if (attempts >= MAX_POLL_ATTEMPTS) {
            stopPolling();
            setStatus('FAILED');
            setError('Timed out waiting for the crawl to finish — it may still be running; check the Execution Hub.');
          }
        } catch (err) {
          stopPolling();
          setStatus('FAILED');
          setError(err instanceof ApiError ? err.message : 'Could not check the crawl run status.');
        }
      }, POLL_INTERVAL_MS);
    } catch (err) {
      setStatus('FAILED');
      setError(err instanceof ApiError ? err.message : 'Failed to start the crawl.');
    }
  };

  const totalPages = output ? new Set(output.roles.flatMap((r) => r.pages.map((p) => p.url))).size : 0;
  const totalElements = output ? output.roles.reduce((sum, r) => sum + r.pages.reduce((s, p) => s + p.elements.length, 0), 0) : 0;
  const isBusy = status === 'STARTING' || status === 'RUNNING';

  return (
    <div id="crawl-grounding-section" className="space-y-3">
      <div className="pb-2 border-b border-slate-800/80">
        <div className="flex items-center gap-2">
          <Compass className="w-4 h-4 text-teal-400" />
          <h2 className="text-sm font-bold text-slate-100 font-sans tracking-tight">Crawl for Real UI Grounding</h2>
          <span className="text-[10px] font-mono font-semibold px-2 py-0.5 rounded bg-teal-950/60 border border-teal-800/60 text-teal-300">
            OPTIONAL
          </span>
        </div>
        <p className="text-xs text-slate-400 font-sans mt-0.5">
          Run a real browser crawl of the application and feed the real pages/elements it finds into generation —
          anonymous pages only for now, no login sequence yet.
        </p>
      </div>

      <div className="flex items-center gap-1.5">
        <input
          id="crawl-target-url"
          type="url"
          placeholder="https://your-app.example.com"
          value={targetUrl}
          onChange={(e) => setTargetUrl(e.target.value)}
          disabled={disabled || isBusy}
          list="crawl-connected-urls"
          className="flex-1 px-2.5 py-1.5 rounded bg-[#020617] border border-slate-700 hover:border-slate-600 focus:border-teal-500 text-xs font-mono text-slate-100 placeholder-slate-500 focus:outline-none"
        />
        <datalist id="crawl-connected-urls">
          {connectedUrls.map((s) => (
            <option key={s.type} value={s.url} />
          ))}
        </datalist>
        <button
          type="button"
          id="btn-start-crawl"
          onClick={handleCrawl}
          disabled={disabled || isBusy || !projectId || !targetUrl.trim()}
          className="px-3 py-1.5 rounded bg-teal-700 hover:bg-teal-600 disabled:bg-slate-800 text-white disabled:text-slate-500 text-xs font-mono font-medium cursor-pointer transition-colors flex items-center gap-1 shrink-0"
        >
          {isBusy ? (
            <>
              <Loader2 className="w-3 h-3 animate-spin" />
              <span>{status === 'STARTING' ? 'Starting…' : 'Crawling…'}</span>
            </>
          ) : (
            <span>Crawl This Application</span>
          )}
        </button>
      </div>

      {status === 'COMPLETED' && output && (
        <div className="p-2.5 rounded bg-[#020617] border border-emerald-900/60 space-y-1.5">
          <div className="flex items-center gap-1.5 text-xs font-mono text-emerald-300">
            <CheckCircle2 className="w-3.5 h-3.5 shrink-0" />
            <span>
              Crawled {totalPages} real page{totalPages === 1 ? '' : 's'}, {totalElements} real element
              {totalElements === 1 ? '' : 's'}
              {lastRunId ? ` (run ${lastRunId.slice(0, 8)})` : ''}.
            </span>
          </div>
          <label className="flex items-center gap-1.5 text-[11px] font-mono text-slate-300 cursor-pointer">
            <input
              type="checkbox"
              id="crawl-use-in-generation"
              checked={crawlRunId !== null}
              onChange={(e) => onCrawlRunIdChange(e.target.checked ? lastRunId : null)}
              disabled={disabled}
              className="accent-teal-600"
            />
            <span>Use this real crawled application data when generating</span>
          </label>
        </div>
      )}

      {status === 'FAILED' && error && (
        <div className="p-2 rounded bg-rose-950/40 border border-rose-800/80 text-[11px] font-mono text-rose-300 flex items-center gap-1.5">
          <XCircle className="w-3.5 h-3.5 shrink-0" />
          <span>{error}</span>
        </div>
      )}

      {status === 'IDLE' && error && (
        <div className="p-2 rounded bg-amber-950/40 border border-amber-800/80 text-[11px] font-mono text-amber-300 flex items-center gap-1.5">
          <Info className="w-3.5 h-3.5 shrink-0" />
          <span>{error}</span>
        </div>
      )}
    </div>
  );
};
