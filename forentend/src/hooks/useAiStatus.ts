/**
 * Real AI engine state (`GET /api/v1/ai/status`) — extracted from
 * `DashboardPage.tsx` so a second page (`TestCaseGeneratorPage.tsx`'s AI
 * Engine rail) can show the same real routing/capability state without a
 * second, independently-drifting copy of the fetch/poll/abort logic.
 *
 * Platform-wide, not project-scoped: loaded once on mount and re-polled on
 * the same slow cadence the AI router itself re-discovers on, regardless of
 * which project (if any) is selected.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { aiqaApi } from '../services/aiqaApi';
import type { AiStatusResponse } from '../types/contract';

export interface UseAiStatusResult {
  status: AiStatusResponse | null;
  loading: boolean;
  /** Real load failure message, if the last fetch failed. */
  error: string | null;
  retry: () => void;
}

const POLL_INTERVAL_MS = 30000;

export function useAiStatus(): UseAiStatusResult {
  const [status, setStatus] = useState<AiStatusResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const load = useCallback(async () => {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setLoading(true);
    try {
      const s = await aiqaApi.ai.status(controller.signal);
      if (controller.signal.aborted) return;
      // Structural guard before this reaches a consumer. A payload that is
      // not actually an `AiStatusResponse` (a proxy error body, an
      // unexpected array) would otherwise crash on `status.providers.map`
      // and take down whatever panel renders it — the same failure mode
      // already fixed in `OrchestratorLogsPanel`. Treated as a real load
      // failure instead.
      if (!s || typeof s !== 'object' || !Array.isArray((s as AiStatusResponse).providers)) {
        setStatus(null);
        setError('The orchestrator returned an unexpected AI status payload.');
        return;
      }
      setStatus(s);
      setError(null);
    } catch (err) {
      if (controller.signal.aborted) return;
      // Never leave a stale engine snapshot on screen looking current.
      setStatus(null);
      setError(err instanceof Error ? err.message : 'Failed to load AI engine status.');
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
    const timer = setInterval(() => void load(), POLL_INTERVAL_MS);
    return () => {
      clearInterval(timer);
      abortRef.current?.abort();
    };
  }, [load]);

  return { status, loading, error, retry: () => void load() };
}
