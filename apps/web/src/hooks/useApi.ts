/**
 * Data-fetching hooks that keep the six real UI states distinguishable.
 *
 * PART 11 of the audit brief: a real application must be able to tell
 * Loading / Empty / Error / Unauthorized / Offline / Not Implemented apart,
 * and must never use fabricated data to paper over any of them. That is
 * enforced structurally here — `useApiResource` has no data parameter to
 * fall back to, so a component physically cannot render numbers the backend
 * did not send.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ApiError, hasApiKey } from "../services/apiClient";

export type ResourceStatus = "loading" | "ready" | "empty" | "error";

export interface ResourceState<T> {
  status: ResourceStatus;
  /** Populated only when status === "ready". Never a placeholder. */
  data: T | null;
  error: ApiError | null;
  /** Re-runs the fetch. Safe to wire directly to a Retry button. */
  reload: () => void;
  /** True while a reload is in flight over already-loaded data. */
  isRefreshing: boolean;
}

/** How a caller declares "this response was successful but has no rows". */
type EmptyCheck<T> = (data: T) => boolean;

const defaultEmptyCheck = <T,>(data: T): boolean => Array.isArray(data) && data.length === 0;

/**
 * Fetches once on mount (and whenever `deps` change), with an AbortController
 * so an unmounted or superseded request can never write stale state.
 *
 * On failure it surfaces the `ApiError` unchanged — including `kind`, which
 * is what lets the view layer choose between the unauthorized, offline and
 * generic-error screens.
 */
export function useApiResource<T>(
  fetcher: (signal: AbortSignal) => Promise<T>,
  deps: unknown[] = [],
  options: { isEmpty?: EmptyCheck<T>; enabled?: boolean } = {},
): ResourceState<T> {
  const { isEmpty = defaultEmptyCheck, enabled = true } = options;

  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [status, setStatus] = useState<ResourceStatus>(enabled ? "loading" : "error");
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [nonce, setNonce] = useState(0);

  // Held in a ref so changing the fetcher identity between renders (an inline
  // arrow, which is the common call shape) does not retrigger the effect.
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;

  const hadDataRef = useRef(false);

  useEffect(() => {
    if (!enabled) {
      setStatus("error");
      setError(new ApiError("unauthenticated", "Not authenticated."));
      return;
    }

    const controller = new AbortController();
    let cancelled = false;

    if (hadDataRef.current) setIsRefreshing(true);
    else setStatus("loading");

    fetcherRef
      .current(controller.signal)
      .then((result) => {
        if (cancelled) return;
        hadDataRef.current = true;
        setData(result);
        setError(null);
        setStatus(isEmpty(result) ? "empty" : "ready");
      })
      .catch((err) => {
        if (cancelled) return;
        if (err instanceof DOMException && err.name === "AbortError") return;
        // Deliberately does NOT keep prior data on screen next to an error —
        // stale numbers presented as current are the same lie as fake ones.
        setData(null);
        setError(err instanceof ApiError ? err : new ApiError("server_error", String(err)));
        setStatus("error");
      })
      .finally(() => {
        if (!cancelled) setIsRefreshing(false);
      });

    return () => {
      cancelled = true;
      controller.abort();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, nonce, enabled]);

  const reload = useCallback(() => setNonce((n) => n + 1), []);

  return { status, data, error, reload, isRefreshing };
}

/**
 * Polls an endpoint on an interval. Used only where the orchestrator has no
 * push channel for that data (health, metrics) — never to animate a counter.
 * Run-level liveness uses the real WebSocket instead (`useRunEvents`).
 */
export function usePolledResource<T>(
  fetcher: (signal: AbortSignal) => Promise<T>,
  intervalMs: number,
  deps: unknown[] = [],
  options: { isEmpty?: EmptyCheck<T>; enabled?: boolean } = {},
): ResourceState<T> {
  const [tick, setTick] = useState(0);

  useEffect(() => {
    if (options.enabled === false) return;
    const id = setInterval(() => setTick((t) => t + 1), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs, options.enabled]);

  return useApiResource(fetcher, [...deps, tick], options);
}

/** Whether the app currently holds an API key. Re-checked on focus so a key
 *  entered in another tab is picked up. */
export function useIsAuthenticated(): boolean {
  const [authed, setAuthed] = useState(hasApiKey);

  useEffect(() => {
    const check = () => setAuthed(hasApiKey());
    window.addEventListener("focus", check);
    window.addEventListener("storage", check);
    return () => {
      window.removeEventListener("focus", check);
      window.removeEventListener("storage", check);
    };
  }, []);

  return authed;
}

/** Mutation helper: tracks in-flight/error state for POST/DELETE actions. */
export function useApiAction<Args extends unknown[], R>(
  action: (...args: Args) => Promise<R>,
): {
  run: (...args: Args) => Promise<R | null>;
  isRunning: boolean;
  error: ApiError | null;
  clearError: () => void;
} {
  const [isRunning, setIsRunning] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  const actionRef = useRef(action);
  actionRef.current = action;

  const run = useCallback(async (...args: Args): Promise<R | null> => {
    setIsRunning(true);
    setError(null);
    try {
      return await actionRef.current(...args);
    } catch (err) {
      setError(err instanceof ApiError ? err : new ApiError("server_error", String(err)));
      return null;
    } finally {
      setIsRunning(false);
    }
  }, []);

  const clearError = useCallback(() => setError(null), []);

  return useMemo(() => ({ run, isRunning, error, clearError }), [run, isRunning, error, clearError]);
}
