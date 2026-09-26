/**
 * Phase 8A — reading what the AI calls came to (`GET /api/v1/projects/:id/ai-usage`, `GET /api/v1/ai/usage`).
 *
 * The report is read when a target (one project, or every project) or a length of time is chosen, and again when asked. What is
 * held belongs to the target and the days it was read for: choosing another never shows the figures of the one before. A refresh keeps
 * showing the report it has until the new one arrives, and a refresh that fails keeps it too, with the error beside it.
 */
import { useCallback, useEffect, useRef, useState } from 'react';
import { ApiError, aiqaApi } from '../services/aiqaApi';
import type { AiUsageReport } from '../types/contract';
import { tzOffsetMinutes } from '../components/ai-usage/aiUsageModel';

/** What to read: one project's usage, or every project's. */
export type UsageTarget = { kind: 'project'; projectId: string } | { kind: 'all' };

export interface AiUsageLook {
  report: AiUsageReport | null;
  /** The last read that failed (cleared by the next one that works). */
  error: ApiError | null;
  /** True until the first read for this target has answered. */
  loading: boolean;
  /** True while a report that is already shown is being read again. */
  refreshing: boolean;
  /** When the report shown was read (ISO time), for the label beside the refresh button. */
  updatedAt: string | null;
  refresh: () => void;
}

interface Held {
  key: string;
  report: AiUsageReport | null;
  error: ApiError | null;
  pending: boolean;
  updatedAt: string | null;
}

const keyOf = (target: UsageTarget | null, days: number): string =>
  target === null ? '' : `${target.kind === 'all' ? 'all' : `project:${target.projectId}`}/${days}`;

/** Nothing read yet: waiting for the first read, unless there is no target (`key` empty), when there is nothing to wait for. */
const nothing = (key: string): Held => ({ key, report: null, error: null, pending: key !== '', updatedAt: null });

export function useAiUsage(target: UsageTarget | null, days: number): AiUsageLook {
  const key = keyOf(target, days);
  const projectId = target?.kind === 'project' ? target.projectId : null;
  const [held, setHeld] = useState<Held>(() => nothing(key));
  const controllerRef = useRef<AbortController | null>(null);

  // What is held belongs to the target and the days it was read for; another target, other days, nothing of it applies.
  const current = held.key === key ? held : nothing(key);

  const read = useCallback(() => {
    if (!key) return;
    controllerRef.current?.abort();
    const controller = new AbortController();
    controllerRef.current = controller;
    setHeld((h) => ({ ...(h.key === key ? h : nothing(key)), pending: true }));
    const query = { days, tz_offset_minutes: tzOffsetMinutes() };
    const request = projectId
      ? aiqaApi.ai.usage.project(projectId, query, controller.signal)
      : aiqaApi.ai.usage.everywhere(query, controller.signal);
    request
      .then((report) => {
        if (!controller.signal.aborted) setHeld({ key, report, error: null, pending: false, updatedAt: new Date().toISOString() });
      })
      .catch((e) => {
        if (controller.signal.aborted) return;
        const error = e instanceof ApiError ? e : new ApiError('server_error', String(e));
        setHeld((h) => ({ ...(h.key === key ? h : nothing(key)), error, pending: false }));
      });
  }, [key, projectId, days]);

  useEffect(() => {
    if (!key) return;
    setHeld(nothing(key));
    read();
    return () => controllerRef.current?.abort();
  }, [key, read]);

  return {
    report: current.report,
    error: current.error,
    loading: current.pending && current.report === null,
    refreshing: current.pending && current.report !== null,
    updatedAt: current.updatedAt,
    refresh: read,
  };
}
