/**
 * Phase 7 — looking at a run as it changes: when the hook reads it, when it reads it again, and what it keeps between looks.
 * `fetch` is mocked at the transport boundary; time is faked so the waiting is exact.
 */
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setApiKey } from '../services/aiqaApi';
import { RETRY_DELAY_MS, useAutopilotRun, useAutopilotRuns } from './useAutopilotRun';
import { PROJECT_ID, RUN_ID, detail, event, run } from '../components/autopilot/autopilotTestData';
import type { AutopilotRunDetail } from '../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}
const errorBody = (message: string) => ({ error: { code: 'INTERNAL_ERROR', message, details: {}, request_id: 'req_hook' } });

/** The run lookups made so far. */
const lookups = (fetchMock: ReturnType<typeof vi.fn>) => fetchMock.mock.calls.filter(([u]: [string]) => String(u).endsWith(`/autopilot/runs/${RUN_ID}`));

/** Lets what is already settled run, without moving the clock. */
const settle = () => act(async () => {});
const tick = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));

beforeEach(() => {
  localStorage.clear();
  setApiKey('aiqa_autopilothook0000');
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  localStorage.clear();
});

describe('useAutopilotRun', () => {
  it('reads nothing while no run is chosen', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAutopilotRun(PROJECT_ID, null));
    await settle();
    await tick(60_000);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(result.current.detail).toBeNull();
  });

  it('reads the run once when it is chosen, and shows it', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, detail('COMPLETED')));
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAutopilotRun(PROJECT_ID, RUN_ID));
    expect(result.current.loading).toBe(true);
    await settle();
    expect(result.current.loading).toBe(false);
    expect(result.current.detail?.run.state).toBe('COMPLETED');
    expect(lookups(fetchMock)).toHaveLength(1);
    expect(String(fetchMock.mock.calls[0][0])).toBe(`/api/v1/projects/${PROJECT_ID}/autopilot/runs/${RUN_ID}`);
  });

  it('reads a running run again every two seconds', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, detail('RUNNING')));
    vi.stubGlobal('fetch', fetchMock);
    renderHook(() => useAutopilotRun(PROJECT_ID, RUN_ID));
    await settle();
    expect(lookups(fetchMock)).toHaveLength(1);
    await tick(1_999);
    expect(lookups(fetchMock)).toHaveLength(1);
    await tick(1);
    expect(lookups(fetchMock)).toHaveLength(2);
    await tick(2_000);
    expect(lookups(fetchMock)).toHaveLength(3);
  });

  it('reads a run that waits for a person, or for its estimate to be accepted, only every ten seconds', async () => {
    for (const state of ['WAITING', 'AWAITING_APPROVAL'] as const) {
      const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, detail(state)));
      vi.stubGlobal('fetch', fetchMock);
      const { unmount } = renderHook(() => useAutopilotRun(PROJECT_ID, RUN_ID));
      await settle();
      await tick(9_999);
      expect(lookups(fetchMock), state).toHaveLength(1);
      await tick(1);
      expect(lookups(fetchMock), state).toHaveLength(2);
      unmount();
    }
  });

  it('stops reading a run once it has ended', async () => {
    for (const state of ['COMPLETED', 'FAILED', 'CANCELLED'] as const) {
      const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, detail(state)));
      vi.stubGlobal('fetch', fetchMock);
      const { unmount } = renderHook(() => useAutopilotRun(PROJECT_ID, RUN_ID));
      await settle();
      await tick(120_000);
      expect(lookups(fetchMock), state).toHaveLength(1);
      unmount();
    }
  });

  it('follows a run from running to completed, and stops when it gets there', async () => {
    const answers = [detail('RUNNING'), detail('RUNNING'), detail('COMPLETED')];
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, answers[Math.min(lookups(fetchMock).length - 1, 2)]));
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAutopilotRun(PROJECT_ID, RUN_ID));
    await settle();
    await tick(2_000);
    await tick(2_000);
    expect(result.current.detail?.run.state).toBe('COMPLETED');
    await tick(60_000);
    expect(lookups(fetchMock)).toHaveLength(3);
  });

  it('keeps every event it has seen, once each and oldest first, when a later look brings only the latest', async () => {
    const first: AutopilotRunDetail = { ...detail('RUNNING'), events: [event(1, 'made'), event(2, 'started'), event(3, 'crawling')] };
    const second: AutopilotRunDetail = { ...detail('RUNNING'), events: [event(3, 'crawling'), event(4, 'crawled')] };
    const answers = [first, second];
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, answers[Math.min(lookups(fetchMock).length - 1, 1)]));
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAutopilotRun(PROJECT_ID, RUN_ID));
    await settle();
    expect(result.current.events.map((e) => e.id)).toEqual([1, 2, 3]);
    await tick(2_000);
    expect(result.current.events.map((e) => e.id)).toEqual([1, 2, 3, 4]);
    expect(result.current.events.map((e) => e.message)).toEqual(['made', 'started', 'crawling', 'crawled']);
  });

  it('shows at once the run an action answered with, and starts reading a run that is running again', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, detail('FAILED')));
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAutopilotRun(PROJECT_ID, RUN_ID));
    await settle();
    await tick(30_000);
    expect(lookups(fetchMock)).toHaveLength(1); // a failed run is not read again

    fetchMock.mockImplementation(async () => jsonResponse(200, detail('RUNNING')));
    act(() => result.current.apply(detail('RUNNING')));
    expect(result.current.detail?.run.state).toBe('RUNNING');
    await tick(2_000);
    expect(lookups(fetchMock)).toHaveLength(2); // and now it is
  });

  it('says a look failed, keeps what it had, and looks again after ten seconds', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(200, detail('RUNNING')))
      .mockResolvedValueOnce(jsonResponse(500, errorBody('the service is busy')))
      .mockResolvedValue(jsonResponse(200, detail('RUNNING', {}, { events: [event(1, 'made'), event(2, 'later')] })));
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAutopilotRun(PROJECT_ID, RUN_ID));
    await settle();
    await tick(2_000);
    expect(result.current.error?.message).toBe('the service is busy');
    expect(result.current.detail?.run.state).toBe('RUNNING');
    expect(lookups(fetchMock)).toHaveLength(2);

    await tick(RETRY_DELAY_MS - 1);
    expect(lookups(fetchMock)).toHaveLength(2);
    await tick(1);
    expect(lookups(fetchMock)).toHaveLength(3);
    expect(result.current.error).toBeNull();
    expect(result.current.events.map((e) => e.id)).toEqual([1, 2]);
  });

  it('reports a first look that failed with no run to show, and reads again when asked', async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(jsonResponse(404, { error: { code: 'RESOURCE_NOT_FOUND', message: 'autopilot run not found in this project', details: {}, request_id: 'r' } }));
    fetchMock.mockResolvedValue(jsonResponse(200, detail('COMPLETED')));
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAutopilotRun(PROJECT_ID, RUN_ID));
    await settle();
    expect(result.current.detail).toBeNull();
    expect(result.current.loading).toBe(false);
    expect(result.current.error?.kind).toBe('not_found');
    await tick(120_000);
    expect(lookups(fetchMock)).toHaveLength(1); // nothing to keep looking at

    act(() => result.current.refresh());
    await settle();
    expect(result.current.error).toBeNull();
    expect(result.current.detail?.run.state).toBe('COMPLETED');
  });

  it('does not show one run under another: choosing a different run starts from nothing and stops reading the first', async () => {
    const OTHER = '33333333-3333-4333-8333-333333333333';
    const fetchMock = vi.fn().mockImplementation(async (url: string) =>
      jsonResponse(200, url.endsWith(OTHER) ? detail('RUNNING', {}, { run: { id: OTHER, name: 'the other run' }, events: [event(7, 'other event')] }) : detail('RUNNING', {}, { events: [event(1, 'first event')] })),
    );
    vi.stubGlobal('fetch', fetchMock);
    const { result, rerender } = renderHook(({ id }) => useAutopilotRun(PROJECT_ID, id), { initialProps: { id: RUN_ID } });
    await settle();
    expect(result.current.detail?.run.name).toBe('Autopilot: https://shop.example.test');

    rerender({ id: OTHER });
    expect(result.current.detail).toBeNull();
    expect(result.current.events).toEqual([]);
    await settle();
    expect(result.current.detail?.run.name).toBe('the other run');
    expect(result.current.events.map((e) => e.message)).toEqual(['other event']);

    fetchMock.mockClear();
    await tick(2_000);
    expect(fetchMock.mock.calls.every(([u]: [string]) => String(u).endsWith(OTHER))).toBe(true);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('stops reading when the page goes away', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, detail('RUNNING')));
    vi.stubGlobal('fetch', fetchMock);
    const { unmount } = renderHook(() => useAutopilotRun(PROJECT_ID, RUN_ID));
    await settle();
    unmount();
    await tick(60_000);
    expect(lookups(fetchMock)).toHaveLength(1);
  });
});

describe('useAutopilotRuns', () => {
  it('lists the project’s runs, and again when asked', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(200, { runs: [run('RUNNING')] }))
      .mockResolvedValue(jsonResponse(200, { runs: [run('COMPLETED')] }));
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAutopilotRuns(PROJECT_ID));
    expect(result.current.loading).toBe(true);
    await settle();
    expect(result.current.runs?.map((r) => r.state)).toEqual(['RUNNING']);
    expect(String(fetchMock.mock.calls[0][0])).toBe(`/api/v1/projects/${PROJECT_ID}/autopilot/runs`);

    act(() => result.current.refresh());
    await settle();
    expect(result.current.runs?.map((r) => r.state)).toEqual(['COMPLETED']);
  });

  it('reads nothing with no project, and shows nothing of the last project under the next', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string) => jsonResponse(200, { runs: [run('COMPLETED', { name: url.includes('proj-b') ? 'B run' : 'A run' })] }));
    vi.stubGlobal('fetch', fetchMock);
    const { result, rerender } = renderHook(({ id }) => useAutopilotRuns(id), { initialProps: { id: null as string | null } });
    await settle();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(result.current.runs).toBeNull();

    rerender({ id: 'proj-a' });
    await settle();
    expect(result.current.runs?.map((r) => r.name)).toEqual(['A run']);

    rerender({ id: 'proj-b' });
    expect(result.current.runs).toBeNull();
    await settle();
    expect(result.current.runs?.map((r) => r.name)).toEqual(['B run']);
  });

  it('says why the runs could not be listed', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(403, { error: { code: 'FORBIDDEN', message: 'not your project', details: {}, request_id: 'r' } })));
    const { result } = renderHook(() => useAutopilotRuns(PROJECT_ID));
    await settle();
    expect(result.current.runs).toBeNull();
    expect(result.current.loading).toBe(false);
    expect(result.current.error?.kind).toBe('forbidden');
  });
});
