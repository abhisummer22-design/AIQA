/**
 * Phase 8B — looking at the report of an OpenAPI test run, and again as long as the run is going.
 *
 * The report is read when a run is chosen and then every `REPORT_POLL_MS` until it says it is final (the run has ended); after that it
 * is not read again by itself. A read that fails is tried again after `RETRY_DELAY_MS`, and the report already shown stays on screen
 * with the error beside it. What is held belongs to the run it was read for: another run, another project, nothing of it applies.
 * `useOpenApiRuns` is the project's list of runs, read when asked.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError, aiqaApi } from '../services/aiqaApi';
import type { OpenApiReport, OpenApiRunListRow } from '../types/contract';
import { REPORT_POLL_MS } from '../components/openapi/openApiModel';

/** How long to wait before reading again after a read that failed. */
export const RETRY_DELAY_MS = 10_000;

export interface OpenApiRunLook {
  report: OpenApiReport | null;
  /** The last read that failed (cleared by the next one that works). */
  error: ApiError | null;
  /** True until the first read of the chosen run has answered. */
  loading: boolean;
  refresh: () => void;
}

interface Held {
  key: string;
  report: OpenApiReport | null;
  error: ApiError | null;
  loading: boolean;
}

const nothing = (key: string): Held => ({ key, report: null, error: null, loading: key !== '' });

const asApiError = (e: unknown): ApiError => (e instanceof ApiError ? e : new ApiError('server_error', String(e)));

export function useOpenApiRun(projectId: string | null, runId: string | null): OpenApiRunLook {
  const key = projectId && runId ? `${projectId}/${runId}` : '';
  const [held, setHeld] = useState<Held>(() => nothing(key));
  // Each answer re-arms the timer of the next read, whether the report changed or not.
  const [stamp, setStamp] = useState(0);
  const controllerRef = useRef<AbortController | null>(null);

  const current = held.key === key ? held : nothing(key);

  const read = useCallback(() => {
    if (!projectId || !runId) return;
    controllerRef.current?.abort();
    const controller = new AbortController();
    controllerRef.current = controller;
    aiqaApi.openapi.runs
      .get(projectId, runId, controller.signal)
      .then((report) => {
        if (controller.signal.aborted) return;
        setHeld({ key, report, error: null, loading: false });
        setStamp((n) => n + 1);
      })
      .catch((e) => {
        if (controller.signal.aborted) return;
        setHeld((h) => ({ ...(h.key === key ? h : nothing(key)), error: asApiError(e), loading: false }));
        setStamp((n) => n + 1);
      });
  }, [projectId, runId, key]);

  useEffect(() => {
    if (!key) return;
    setHeld(nothing(key));
    read();
    return () => controllerRef.current?.abort();
  }, [key, read]);

  // The next read: soon while the run is going, later after a failure, never once the report is final.
  const final = current.report?.final ?? false;
  const failed = current.error !== null;
  const shown = current.report !== null;
  useEffect(() => {
    if ((!shown && !failed) || final) return;
    const timer = setTimeout(read, failed ? RETRY_DELAY_MS : REPORT_POLL_MS);
    return () => clearTimeout(timer);
  }, [key, shown, failed, final, stamp, read]);

  return { report: current.report, error: current.error, loading: current.loading, refresh: read };
}

export interface OpenApiRunsList {
  runs: OpenApiRunListRow[] | null;
  error: ApiError | null;
  loading: boolean;
  refresh: () => void;
}

export function useOpenApiRuns(projectId: string | null): OpenApiRunsList {
  const [held, setHeld] = useState<{ projectId: string | null; runs: OpenApiRunListRow[] | null; error: ApiError | null; loading: boolean }>({
    projectId,
    runs: null,
    error: null,
    loading: projectId !== null,
  });
  const controllerRef = useRef<AbortController | null>(null);

  const refresh = useCallback(() => {
    if (!projectId) return;
    controllerRef.current?.abort();
    const controller = new AbortController();
    controllerRef.current = controller;
    aiqaApi.openapi.runs
      .list(projectId, controller.signal)
      .then(({ runs }) => {
        if (!controller.signal.aborted) setHeld({ projectId, runs, error: null, loading: false });
      })
      .catch((e) => {
        if (controller.signal.aborted) return;
        setHeld((h) => ({ projectId, runs: h.projectId === projectId ? h.runs : null, error: asApiError(e), loading: false }));
      });
  }, [projectId]);

  useEffect(() => {
    if (!projectId) return;
    setHeld({ projectId, runs: null, error: null, loading: true });
    refresh();
    return () => controllerRef.current?.abort();
  }, [projectId, refresh]);

  const current = held.projectId === projectId ? held : { projectId, runs: null, error: null, loading: projectId !== null };
  return { runs: current.runs, error: current.error, loading: current.loading, refresh };
}
