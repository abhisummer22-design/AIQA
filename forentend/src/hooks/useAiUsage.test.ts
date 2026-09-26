/**
 * Phase 8A — reading what the AI calls came to: what is asked for, when it is read again, and what is kept between reads.
 * `fetch` is mocked at the transport boundary.
 */
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setApiKey } from '../services/aiqaApi';
import { useAiUsage, type UsageTarget } from './useAiUsage';
import { PROJECT_ID, usageReport } from '../components/ai-usage/aiUsageTestData';
import { backend, failure, requests } from '../components/autopilot/autopilotTestBackend';
import type { AiUsageReport } from '../types/contract';

const PROJECT: UsageTarget = { kind: 'project', projectId: PROJECT_ID };
const EVERYWHERE: UsageTarget = { kind: 'all' };

/** Lets what is already settled run. */
const settle = () => act(async () => {});

beforeEach(() => {
  localStorage.clear();
  setApiKey('aiqa_aiusagehook0000');
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  vi.unstubAllGlobals();
  localStorage.clear();
});

describe('useAiUsage', () => {
  it('reads nothing while there is no target, and is not loading anything', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAiUsage(null, 30));
    await settle();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(result.current).toMatchObject({ report: null, error: null, loading: false, refreshing: false, updatedAt: null });
  });

  it('reads a project’s usage for the days chosen, on the viewer’s own clock', async () => {
    vi.spyOn(Date.prototype, 'getTimezoneOffset').mockReturnValue(-330);
    const fetchMock = backend({ 'GET /ai-usage': () => usageReport() });
    const { result } = renderHook(() => useAiUsage(PROJECT, 7));
    expect(result.current.loading).toBe(true);
    await settle();
    const asked = requests(fetchMock, 'GET /ai-usage');
    expect(asked).toHaveLength(1);
    expect(asked[0].path).toBe(`/api/v1/projects/${PROJECT_ID}/ai-usage`);
    expect(asked[0].query).toEqual({ days: '7', tz_offset_minutes: '330' });
    expect(result.current.report).toEqual(usageReport());
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
    expect(result.current.updatedAt).toMatch(/^\d{4}-\d{2}-\d{2}T/);
  });

  it('reads every project’s usage from the platform-wide route', async () => {
    const fetchMock = backend({ 'GET /ai/usage': () => usageReport() });
    const { result } = renderHook(() => useAiUsage(EVERYWHERE, 30));
    await settle();
    const asked = requests(fetchMock, 'GET /ai/usage');
    expect(asked).toHaveLength(1);
    expect(asked[0].path).toBe('/api/v1/ai/usage');
    expect(asked[0].query.days).toBe('30');
    expect(result.current.report).not.toBeNull();
  });

  it('reads again for other days, and never shows the figures of the days before while it waits', async () => {
    let release: (r: unknown) => void = () => {};
    let reads = 0;
    backend({
      'GET /ai-usage': (req) => {
        reads += 1;
        return req.query.days === '7'
          ? usageReport({ range: { days: 7, from: '2026-09-20', to: '2026-09-26', tz_offset_minutes: 0 } })
          : new Promise((resolve) => (release = resolve));
      },
    });
    const seen: Array<{ days: number; report: AiUsageReport | null; loading: boolean }> = [];
    const { rerender } = renderHook(
      ({ days }) => {
        const look = useAiUsage(PROJECT, days);
        seen.push({ days, report: look.report, loading: look.loading });
        return look;
      },
      { initialProps: { days: 7 } },
    );
    await settle();
    rerender({ days: 30 });
    await settle();
    expect(reads).toBe(2);
    // Every render for 30 days, until its own answer arrives, has no report and says it is loading.
    const forThirty = seen.filter((s) => s.days === 30);
    expect(forThirty.length).toBeGreaterThan(0);
    expect(forThirty.every((s) => s.report === null && s.loading)).toBe(true);
    await act(async () => release(usageReport({ range: { days: 30, from: '2026-08-28', to: '2026-09-26', tz_offset_minutes: 0 } })));
    expect(seen.at(-1)?.report?.range.days).toBe(30);
    expect(seen.at(-1)?.loading).toBe(false);
  });

  it('reads again for another project, and forgets the first', async () => {
    const fetchMock = backend({ 'GET /ai-usage': () => usageReport() });
    const { result, rerender } = renderHook(({ target }: { target: UsageTarget }) => useAiUsage(target, 30), { initialProps: { target: PROJECT } });
    await settle();
    rerender({ target: { kind: 'project', projectId: 'proj-b' } });
    expect(result.current.report).toBeNull();
    expect(result.current.loading).toBe(true);
    await settle();
    expect(requests(fetchMock, 'GET /ai-usage').map((r) => r.path)).toEqual([`/api/v1/projects/${PROJECT_ID}/ai-usage`, '/api/v1/projects/proj-b/ai-usage']);
    expect(result.current.report).not.toBeNull();
  });

  it('never shows one project’s figures for another, not even for the one render before it starts reading', async () => {
    backend({
      'GET /ai-usage': (req) => usageReport({ notes: [req.path.includes('proj-b') ? 'B' : 'A'] }),
      'GET /ai/usage': () => usageReport({ notes: ['ALL'] }),
    });
    const seen: Array<{ project: string; note: string | null }> = [];
    const { rerender } = renderHook(
      ({ target }: { target: UsageTarget }) => {
        const look = useAiUsage(target, 30);
        seen.push({ project: target.kind === 'project' ? target.projectId : 'all', note: look.report?.notes[0] ?? null });
        return look;
      },
      { initialProps: { target: PROJECT } },
    );
    await settle();
    rerender({ target: { kind: 'project', projectId: 'proj-b' } });
    await settle();
    rerender({ target: EVERYWHERE });
    await settle();
    // Every render for a target shows nothing of another one's, and the first for each is empty.
    const shownFor = (project: string) => seen.filter((s) => s.project === project).map((s) => s.note);
    expect(shownFor('proj-b').filter((note) => note !== null && note !== 'B')).toEqual([]);
    expect(shownFor('all').filter((note) => note !== null && note !== 'ALL')).toEqual([]);
    expect(shownFor('proj-b')[0]).toBeNull();
    expect(shownFor('all')[0]).toBeNull();
    expect(shownFor('proj-b').at(-1)).toBe('B');
    expect(shownFor('all').at(-1)).toBe('ALL');
  });

  it('ignores an answer that arrives after another target was chosen', async () => {
    const late: Array<(r: unknown) => void> = [];
    backend({
      'GET /ai-usage': (req) =>
        req.path.includes(PROJECT_ID) ? new Promise((resolve) => late.push(resolve)) : usageReport({ notes: ['the second project'] }),
    });
    const { result, rerender } = renderHook(({ target }: { target: UsageTarget }) => useAiUsage(target, 30), { initialProps: { target: PROJECT } });
    await settle();
    rerender({ target: { kind: 'project', projectId: 'proj-b' } });
    await settle();
    await act(async () => late[0](usageReport({ notes: ['the first project'] })));
    expect(result.current.report?.notes).toEqual(['the second project']);
  });

  it('keeps showing the report it has while it reads again, and says it is refreshing', async () => {
    let release: (r: unknown) => void = () => {};
    let reads = 0;
    backend({
      'GET /ai-usage': () => (++reads === 1 ? usageReport({ notes: ['before'] }) : new Promise((resolve) => (release = resolve))),
    });
    const { result } = renderHook(() => useAiUsage(PROJECT, 30));
    await settle();
    const firstRead = result.current.updatedAt;
    act(() => result.current.refresh());
    await settle();
    expect(result.current.report?.notes).toEqual(['before']);
    expect(result.current.refreshing).toBe(true);
    expect(result.current.loading).toBe(false);
    await act(async () => release(usageReport({ notes: ['after'] })));
    expect(result.current.report?.notes).toEqual(['after']);
    expect(result.current.refreshing).toBe(false);
    expect(result.current.updatedAt).not.toBeNull();
    expect(firstRead).not.toBeNull();
  });

  it('says so when the first read fails, with nothing to show', async () => {
    backend({ 'GET /ai-usage': () => failure(500, 'INTERNAL_ERROR', 'the ledger is busy') });
    const { result } = renderHook(() => useAiUsage(PROJECT, 30));
    await settle();
    expect(result.current.report).toBeNull();
    expect(result.current.loading).toBe(false);
    expect(result.current.error?.message).toContain('the ledger is busy');
  });

  it('keeps the report when a later read fails, with the error beside it, and clears the error when a read works again', async () => {
    let reads = 0;
    backend({
      'GET /ai-usage': () => {
        reads += 1;
        return reads === 2 ? failure(500, 'INTERNAL_ERROR', 'gone for a moment') : usageReport({ notes: [`read ${reads}`] });
      },
    });
    const { result } = renderHook(() => useAiUsage(PROJECT, 30));
    await settle();
    act(() => result.current.refresh());
    await settle();
    expect(result.current.report?.notes).toEqual(['read 1']);
    expect(result.current.error?.message).toContain('gone for a moment');
    expect(result.current.refreshing).toBe(false);
    act(() => result.current.refresh());
    await settle();
    expect(result.current.report?.notes).toEqual(['read 3']);
    expect(result.current.error).toBeNull();
  });

  it('wraps a failure that is not the transport’s own as a server error', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('boom')));
    const { result } = renderHook(() => useAiUsage(PROJECT, 30));
    await settle();
    expect(result.current.error).not.toBeNull();
    expect(result.current.report).toBeNull();
    expect(result.current.loading).toBe(false);
  });

  it('stops reading when it is left', async () => {
    let signal: AbortSignal | undefined;
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation((_url: string, init?: RequestInit) => {
        signal = init?.signal ?? undefined;
        return new Promise(() => {});
      }),
    );
    const { unmount } = renderHook(() => useAiUsage(PROJECT, 30));
    await settle();
    expect(signal?.aborted).toBe(false);
    unmount();
    expect(signal?.aborted).toBe(true);
  });

  it('a read for the same target that is asked for again replaces the one in flight', async () => {
    const signals: AbortSignal[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation((_url: string, init?: RequestInit) => {
        signals.push(init!.signal!);
        return new Promise(() => {});
      }),
    );
    const { result } = renderHook(() => useAiUsage(PROJECT, 30));
    await settle();
    act(() => result.current.refresh());
    await settle();
    expect(signals).toHaveLength(2);
    expect(signals[0].aborted).toBe(true);
    expect(signals[1].aborted).toBe(false);
  });
});
