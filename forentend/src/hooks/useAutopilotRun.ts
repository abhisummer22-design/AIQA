/**
 * Phase 7 — looking at one autopilot run, and again as long as it is changing.
 *
 * The run lives in the service (its database is its memory), so this only reads it: once when a run is chosen, then again after the
 * time `pollDelayMs` gives for its state (often while it runs, seldom while it waits for a person, never once it has ended). An action
 * on the run answers with the run as it then is, which `apply` shows at once and which starts the looking again (a failed run that is
 * started again is running, and is looked at often once more). The timeline is every event seen so far, merged by id, so it does not
 * lose the early ones when a later look brings only the latest.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError, aiqaApi } from '../services/aiqaApi';
import type { AutopilotEvent, AutopilotRunDetail } from '../types/contract';
import { mergeEvents, pollDelayMs } from '../components/autopilot/autopilotModel';

/** How long to wait before looking again after a look that failed. */
export const RETRY_DELAY_MS = 10_000;

export interface AutopilotRunLook {
  detail: AutopilotRunDetail | null;
  events: AutopilotEvent[];
  /** The last look that failed (cleared by the next one that works). */
  error: ApiError | null;
  /** True until the first look at the chosen run has answered. */
  loading: boolean;
  refresh: () => void;
  /** Shows a run an action has just answered with. */
  apply: (detail: AutopilotRunDetail) => void;
}

interface Held {
  key: string;
  detail: AutopilotRunDetail | null;
  events: AutopilotEvent[];
  error: ApiError | null;
  loading: boolean;
}

/** Nothing read yet: loading, unless there is no run chosen (`key` empty), when there is nothing to load. */
const nothing = (key: string): Held => ({ key, detail: null, events: [], error: null, loading: key !== '' });

export function useAutopilotRun(projectId: string | null, runId: string | null): AutopilotRunLook {
  const key = projectId && runId ? `${projectId}/${runId}` : '';
  const [held, setHeld] = useState<Held>(() => nothing(key));
  // Each answer re-arms the timer of the next look, whether it changed the run's state or not.
  const [stamp, setStamp] = useState(0);
  const controllerRef = useRef<AbortController | null>(null);

  // What is held belongs to the run it was read for: another run, another project, nothing of it applies.
  const current = held.key === key ? held : nothing(key);

  const apply = useCallback(
    (detail: AutopilotRunDetail) => {
      setHeld((h) => ({
        key,
        detail,
        events: mergeEvents(h.key === key ? h.events : [], detail.events),
        error: null,
        loading: false,
      }));
      setStamp((n) => n + 1);
    },
    [key],
  );

  const look = useCallback(() => {
    if (!projectId || !runId) return;
    controllerRef.current?.abort();
    const controller = new AbortController();
    controllerRef.current = controller;
    aiqaApi.autopilot.runs
      .get(projectId, runId, controller.signal)
      .then((detail) => {
        if (!controller.signal.aborted) apply(detail);
      })
      .catch((e) => {
        if (controller.signal.aborted) return;
        const error = e instanceof ApiError ? e : new ApiError('server_error', String(e));
        setHeld((h) => ({ ...(h.key === key ? h : nothing(key)), error, loading: false }));
        setStamp((n) => n + 1);
      });
  }, [projectId, runId, key, apply]);

  // A run is chosen (or the project changes): look at it, and stop looking at the one before.
  useEffect(() => {
    if (!key) return;
    setHeld(nothing(key));
    look();
    return () => controllerRef.current?.abort();
  }, [key, look]);

  // The next look, after the time its state asks for; after a look that failed, after the retry delay.
  const state = current.detail?.run.state ?? null;
  const failed = current.error !== null;
  useEffect(() => {
    if (!key || !state) return;
    // A run that has ended will not change, so a look that failed is not tried again by itself (the person can ask).
    const delay = pollDelayMs(state);
    if (delay === null) return;
    const timer = setTimeout(look, failed ? RETRY_DELAY_MS : delay);
    return () => clearTimeout(timer);
  }, [key, state, failed, stamp, look]);

  return { detail: current.detail, events: current.events, error: current.error, loading: current.loading, refresh: look, apply };
}

/** The runs of a project, newest first, read again when asked (after a run is made, or the chosen one changes state). */
export interface AutopilotRunsList {
  runs: AutopilotRunDetail['run'][] | null;
  error: ApiError | null;
  loading: boolean;
  refresh: () => void;
}

export function useAutopilotRuns(projectId: string | null): AutopilotRunsList {
  const [held, setHeld] = useState<{ projectId: string | null; runs: AutopilotRunDetail['run'][] | null; error: ApiError | null; loading: boolean }>({
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
    aiqaApi.autopilot.runs
      .list(projectId, controller.signal)
      .then(({ runs }) => {
        if (!controller.signal.aborted) setHeld({ projectId, runs, error: null, loading: false });
      })
      .catch((e) => {
        if (controller.signal.aborted) return;
        const error = e instanceof ApiError ? e : new ApiError('server_error', String(e));
        setHeld((h) => ({ projectId, runs: h.projectId === projectId ? h.runs : null, error, loading: false }));
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
