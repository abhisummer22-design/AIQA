/**
 * Phase 8C — something read from the service when a key is chosen and read again after a delay it names, for as long as it says to.
 *
 * `key` says what is read ('' reads nothing) and `load` reads it. After each answer `next` says how long to wait before reading again
 * (`null`: not again by itself); a read that failed is told to `next` as `failed`, and what was held stays on screen beside the error.
 * What is held belongs to its key: another key, nothing of it applies (no flash of the last one while the new one is read).
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError } from '../services/aiqaApi';

export interface Polled<T> {
  value: T | null;
  /** The last read that failed (cleared by the next one that works). */
  error: ApiError | null;
  /** True until the first read of the chosen key has answered. */
  loading: boolean;
  refresh: () => void;
}

interface Held<T> {
  key: string;
  value: T | null;
  error: ApiError | null;
  loading: boolean;
}

function nothing<T>(key: string): Held<T> {
  return { key, value: null, error: null, loading: key !== '' };
}

const asApiError = (e: unknown): ApiError => (e instanceof ApiError ? e : new ApiError('server_error', String(e)));

export function usePolled<T>(key: string, load: (signal: AbortSignal) => Promise<T>, next: (value: T | null, failed: boolean) => number | null): Polled<T> {
  const [held, setHeld] = useState<Held<T>>(() => nothing<T>(key));
  // Each answer re-arms the timer of the next read, whether what was read changed or not.
  const [stamp, setStamp] = useState(0);
  const controllerRef = useRef<AbortController | null>(null);
  const loadRef = useRef(load);
  loadRef.current = load;
  const nextRef = useRef(next);
  nextRef.current = next;

  const current = held.key === key ? held : nothing<T>(key);

  const read = useCallback(() => {
    if (!key) return;
    controllerRef.current?.abort();
    const controller = new AbortController();
    controllerRef.current = controller;
    loadRef
      .current(controller.signal)
      .then((value) => {
        if (controller.signal.aborted) return;
        setHeld({ key, value, error: null, loading: false });
        setStamp((n) => n + 1);
      })
      .catch((e) => {
        if (controller.signal.aborted) return;
        setHeld((h) => ({ ...(h.key === key ? h : nothing<T>(key)), error: asApiError(e), loading: false }));
        setStamp((n) => n + 1);
      });
  }, [key]);

  useEffect(() => {
    if (!key) return;
    setHeld(nothing<T>(key));
    read();
    return () => controllerRef.current?.abort();
  }, [key, read]);

  const failed = current.error !== null;
  useEffect(() => {
    // Nothing has answered yet: the answer arms the next read.
    if (!key || (current.value === null && !failed)) return;
    const delay = nextRef.current(current.value, failed);
    if (delay === null) return;
    const timer = setTimeout(read, delay);
    return () => clearTimeout(timer);
  }, [key, current.value, failed, stamp, read]);

  return { value: current.value, error: current.error, loading: current.loading, refresh: read };
}
