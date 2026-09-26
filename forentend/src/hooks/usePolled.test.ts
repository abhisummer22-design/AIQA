/**
 * Phase 8C — reading something when a key is chosen and again after a delay, for as long as it says to; and the three things the
 * scheduled-regression page reads with it. `fetch` is mocked at the transport boundary; time is faked so the waiting is exact.
 */
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError, setApiKey } from '../services/aiqaApi';
import { usePolled } from './usePolled';
import { useRegressionReport, useRegressionRuns, useRegressionSchedules } from './useRegression';
import { REPORT_POLL_MS, RETRY_DELAY_MS, RUNS_IDLE_POLL_MS, RUNS_POLL_MS, SCHEDULE_POLL_MS } from '../components/regression/regressionModel';
import { PROJECT_ID, RUN_ID, SCHEDULE_ID, accountFor, reportFor, scheduleFor } from '../components/regression/regressionTestData';
import { backend, failure, requests } from '../components/autopilot/autopilotTestBackend';

const settle = () => act(async () => {});
const tick = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));

beforeEach(() => {
  localStorage.clear();
  setApiKey('aiqa_regressionhook000');
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  localStorage.clear();
});

describe('usePolled', () => {
  it('reads nothing for an empty key, and is not loading', async () => {
    const load = vi.fn();
    const { result } = renderHook(() => usePolled('', load, () => 1000));
    await settle();
    await tick(60_000);
    expect(load).not.toHaveBeenCalled();
    expect(result.current).toMatchObject({ value: null, error: null, loading: false });
  });

  it('reads once when the key is chosen, says it is loading until the answer, and again after the delay it is given', async () => {
    const load = vi.fn().mockImplementation(async () => load.mock.calls.length);
    const next = vi.fn().mockReturnValue(1000);
    const { result } = renderHook(() => usePolled('k', load, next));
    expect(result.current.loading).toBe(true);
    await settle();
    expect(load).toHaveBeenCalledTimes(1);
    expect(result.current).toMatchObject({ value: 1, loading: false, error: null });
    expect(next).toHaveBeenLastCalledWith(1, false);
    await tick(999);
    expect(load).toHaveBeenCalledTimes(1);
    await tick(1);
    expect(load).toHaveBeenCalledTimes(2);
    expect(result.current.value).toBe(2);
    await tick(1000);
    expect(load).toHaveBeenCalledTimes(3);
  });

  it('stops when it is told not to read again', async () => {
    const load = vi.fn().mockResolvedValue('done');
    renderHook(() => usePolled('k', load, () => null));
    await settle();
    await tick(600_000);
    expect(load).toHaveBeenCalledTimes(1);
  });

  it('keeps what it has when a later read fails, says so, and reads again after the delay for a failure', async () => {
    let reads = 0;
    const load = vi.fn().mockImplementation(async () => {
      reads += 1;
      if (reads === 2) throw new ApiError('server_error', 'gone for a moment');
      return reads;
    });
    const next = vi.fn((_v: number | null, failed: boolean) => (failed ? 5000 : 1000));
    const { result } = renderHook(() => usePolled('k', load, next));
    await settle();
    await tick(1000);
    expect(reads).toBe(2);
    expect(result.current.value).toBe(1);
    expect(result.current.error?.message).toContain('gone for a moment');
    expect(next).toHaveBeenLastCalledWith(1, true);
    await tick(4999);
    expect(reads).toBe(2);
    await tick(1);
    expect(reads).toBe(3);
    expect(result.current.error).toBeNull();
    expect(result.current.value).toBe(3);
  });

  it('says so when the first read fails, with nothing to show, and tries again after the delay', async () => {
    let reads = 0;
    const load = vi.fn().mockImplementation(async () => {
      if (++reads === 1) throw new Error('offline');
      return 'ok';
    });
    const { result } = renderHook(() => usePolled('k', load, (_v, failed) => (failed ? 3000 : null)));
    await settle();
    expect(result.current).toMatchObject({ value: null, loading: false });
    expect(result.current.error?.message).toContain('offline');
    await tick(3000);
    expect(result.current.value).toBe('ok');
    expect(result.current.error).toBeNull();
  });

  it('holds what it read for its own key: another key shows nothing of the first while it is read', async () => {
    const load = vi.fn();
    const seen: Array<{ key: string; value: string | null; loading: boolean }> = [];
    const { rerender } = renderHook(
      ({ key }) => {
        const look = usePolled(key, async () => `value of ${key}`, () => null);
        seen.push({ key, value: look.value, loading: look.loading });
        return look;
      },
      { initialProps: { key: 'a' } },
    );
    void load;
    await settle();
    rerender({ key: 'b' });
    await settle();
    expect(seen.filter((s) => s.key === 'b' && s.value === 'value of a')).toEqual([]);
    expect(seen.find((s) => s.key === 'b')).toEqual({ key: 'b', value: null, loading: true });
    expect(seen.at(-1)).toEqual({ key: 'b', value: 'value of b', loading: false });
  });

  it('ignores an answer that comes after another key was chosen', async () => {
    const late: Array<(v: string) => void> = [];
    const { result, rerender } = renderHook(({ key }) => usePolled(key, (): Promise<string> => (key === 'a' ? new Promise((r) => late.push(r)) : Promise.resolve('b')), () => null), {
      initialProps: { key: 'a' },
    });
    await settle();
    rerender({ key: 'b' });
    await settle();
    await act(async () => late[0]('a, late'));
    expect(result.current.value).toBe('b');
  });

  it('can be asked to read again, and keeps what it has while it does', async () => {
    let release: (v: string) => void = () => {};
    let reads = 0;
    const load = vi.fn().mockImplementation(() => (++reads === 1 ? Promise.resolve('before') : new Promise<string>((r) => (release = r))));
    const { result } = renderHook(() => usePolled('k', load, () => null));
    await settle();
    act(() => result.current.refresh());
    await settle();
    expect(result.current.value).toBe('before');
    await act(async () => release('after'));
    expect(result.current.value).toBe('after');
  });

  it('reads nothing when asked to read again with no key', async () => {
    const load = vi.fn();
    const { result } = renderHook(() => usePolled('', load, () => 1000));
    act(() => result.current.refresh());
    await settle();
    expect(load).not.toHaveBeenCalled();
  });

  it('stops reading when it is left', async () => {
    let signal: AbortSignal | undefined;
    const load = vi.fn().mockImplementation((s: AbortSignal) => {
      signal = s;
      return new Promise(() => {});
    });
    const { unmount } = renderHook(() => usePolled('k', load, () => 1000));
    await settle();
    expect(signal?.aborted).toBe(false);
    unmount();
    expect(signal?.aborted).toBe(true);
  });
});

describe('what the page reads', () => {
  it('the project’s schedules: every fifteen seconds, or ten after a failure', async () => {
    let reads = 0;
    const fetchMock = backend({ 'GET /regression/schedules$': () => (++reads === 2 ? failure(500, 'INTERNAL_ERROR', 'busy') : { schedules: [scheduleFor()] }) });
    const { result } = renderHook(() => useRegressionSchedules(PROJECT_ID));
    await settle();
    expect(requests(fetchMock, 'GET /regression/schedules$')[0].path).toBe(`/api/v1/projects/${PROJECT_ID}/regression/schedules`);
    expect(result.current.value?.[0].name).toBe('Nightly');
    await tick(SCHEDULE_POLL_MS - 1);
    expect(reads).toBe(1);
    await tick(1);
    expect(reads).toBe(2);
    expect(result.current.error?.message).toContain('busy');
    await tick(RETRY_DELAY_MS);
    expect(reads).toBe(3);
    expect(result.current.error).toBeNull();
  });

  it('nothing with no project', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderHook(() => useRegressionSchedules(null));
    renderHook(() => useRegressionRuns(PROJECT_ID, null));
    renderHook(() => useRegressionRuns(null, SCHEDULE_ID));
    renderHook(() => useRegressionReport(PROJECT_ID, null));
    await settle();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('a schedule’s runs: every three seconds while one is going, every fifteen when none is', async () => {
    let reads = 0;
    const fetchMock = backend({ [`GET /regression/schedules/${SCHEDULE_ID}/runs$`]: () => ({ runs: [accountFor({ final: ++reads >= 2 })] }) });
    const { result } = renderHook(() => useRegressionRuns(PROJECT_ID, SCHEDULE_ID));
    await settle();
    expect(requests(fetchMock, `GET /regression/schedules/${SCHEDULE_ID}/runs$`)[0].path).toBe(`/api/v1/projects/${PROJECT_ID}/regression/schedules/${SCHEDULE_ID}/runs`);
    expect(result.current.value?.[0].final).toBe(false);
    await tick(RUNS_POLL_MS);
    expect(reads).toBe(2);
    expect(result.current.value?.[0].final).toBe(true);
    await tick(RUNS_POLL_MS);
    expect(reads, 'nothing is going: not as often').toBe(2);
    await tick(RUNS_IDLE_POLL_MS - RUNS_POLL_MS);
    expect(reads).toBe(3);
  });

  it('a run’s report: every two seconds until the run has ended, and then not again', async () => {
    let reads = 0;
    const fetchMock = backend({ [`GET /regression/runs/${RUN_ID}$`]: () => reportFor({ final: ++reads >= 3, verdict: reads >= 3 ? 'PASSING' : 'RUNNING' }) });
    const { result } = renderHook(() => useRegressionReport(PROJECT_ID, RUN_ID));
    await settle();
    expect(requests(fetchMock, `GET /regression/runs/${RUN_ID}$`)[0].path).toBe(`/api/v1/projects/${PROJECT_ID}/regression/runs/${RUN_ID}`);
    await tick(REPORT_POLL_MS - 1);
    expect(reads).toBe(1);
    await tick(1);
    expect(reads).toBe(2);
    await tick(REPORT_POLL_MS);
    expect(reads).toBe(3);
    expect(result.current.value?.final).toBe(true);
    await tick(600_000);
    expect(reads, 'a run that has ended is not read again').toBe(3);
  });
});
