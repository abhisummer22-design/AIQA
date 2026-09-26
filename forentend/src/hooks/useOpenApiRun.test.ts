/**
 * Phase 8B — looking at the report of an OpenAPI test run as it changes: when the hook reads it, when it reads it again, and what it keeps
 * between reads. `fetch` is mocked at the transport boundary; time is faked so the waiting is exact.
 */
import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setApiKey } from '../services/aiqaApi';
import { RETRY_DELAY_MS, useOpenApiRun, useOpenApiRuns } from './useOpenApiRun';
import { OTHER_RUN_ID, PROJECT_ID, RUN_ID, reportFor, runRow } from '../components/openapi/openApiTestData';
import { REPORT_POLL_MS } from '../components/openapi/openApiModel';
import { backend, failure, requests } from '../components/autopilot/autopilotTestBackend';

const settle = () => act(async () => {});
const tick = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));

beforeEach(() => {
  localStorage.clear();
  setApiKey('aiqa_openapihook00000');
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  localStorage.clear();
});

const READ_ROUTE = `GET /openapi/runs/${RUN_ID}$`;

describe('useOpenApiRun', () => {
  it('waits ten seconds before it reads again after a read that failed', () => {
    expect(RETRY_DELAY_MS).toBe(10_000);
  });

  it('reads nothing while no run is chosen, and is not loading anything', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useOpenApiRun(PROJECT_ID, null));
    await settle();
    await tick(60_000);
    expect(fetchMock).not.toHaveBeenCalled();
    expect(result.current).toMatchObject({ report: null, error: null, loading: false });
  });

  it('reads nothing with no project either', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useOpenApiRun(null, RUN_ID));
    await settle();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(result.current.loading).toBe(false);
  });

  it('reads the chosen run’s report once, and says it is loading until it has', async () => {
    const fetchMock = backend({ [READ_ROUTE]: () => reportFor() });
    const { result } = renderHook(() => useOpenApiRun(PROJECT_ID, RUN_ID));
    expect(result.current.loading).toBe(true);
    await settle();
    expect(requests(fetchMock, READ_ROUTE)).toHaveLength(1);
    expect(requests(fetchMock, READ_ROUTE)[0].path).toBe(`/api/v1/projects/${PROJECT_ID}/openapi/runs/${RUN_ID}`);
    expect(result.current.report?.spec.title).toBe('Fixture Pets');
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('reads again every two seconds while the run is going, and stops when the report is final', async () => {
    let reads = 0;
    const fetchMock = backend({
      [READ_ROUTE]: () => reportFor({ final: ++reads >= 3, summary: { ...reportFor().summary, pending: reads >= 3 ? 0 : 5 } }),
    });
    const { result } = renderHook(() => useOpenApiRun(PROJECT_ID, RUN_ID));
    await settle();
    expect(reads).toBe(1);
    await tick(REPORT_POLL_MS - 1);
    expect(reads).toBe(1);
    await tick(1);
    expect(reads).toBe(2);
    expect(result.current.report?.final).toBe(false);
    await tick(REPORT_POLL_MS);
    expect(reads).toBe(3);
    expect(result.current.report?.final).toBe(true);
    await tick(60_000);
    expect(reads, 'a run that has ended is not read again').toBe(3);
    expect(requests(fetchMock, READ_ROUTE)).toHaveLength(3);
  });

  it('keeps the report it has when a later read fails, says so, and reads again after the retry delay', async () => {
    let reads = 0;
    backend({
      [READ_ROUTE]: () => {
        reads += 1;
        if (reads === 2) return failure(500, 'INTERNAL_ERROR', 'gone for a moment');
        return reportFor({ final: reads >= 3 });
      },
    });
    const { result } = renderHook(() => useOpenApiRun(PROJECT_ID, RUN_ID));
    await settle();
    await tick(REPORT_POLL_MS);
    expect(reads).toBe(2);
    expect(result.current.report).not.toBeNull();
    expect(result.current.error?.message).toContain('gone for a moment');
    await tick(RETRY_DELAY_MS - 1);
    expect(reads).toBe(2);
    await tick(1);
    expect(reads).toBe(3);
    expect(result.current.error).toBeNull();
    expect(result.current.report?.final).toBe(true);
  });

  it('says so when the first read fails, with nothing to show, and tries again after the retry delay', async () => {
    let reads = 0;
    backend({ [READ_ROUTE]: () => (++reads === 1 ? failure(404, 'RESOURCE_NOT_FOUND', 'OpenAPI test run not found in this project') : reportFor()) });
    const { result } = renderHook(() => useOpenApiRun(PROJECT_ID, RUN_ID));
    await settle();
    expect(result.current.report).toBeNull();
    expect(result.current.loading).toBe(false);
    expect(result.current.error?.message).toContain('not found');
    await tick(RETRY_DELAY_MS);
    expect(reads).toBe(2);
    expect(result.current.report).not.toBeNull();
    expect(result.current.error).toBeNull();
  });

  it('reads another run’s report when another is chosen, and never shows the first run’s for it', async () => {
    backend({
      [READ_ROUTE]: () => reportFor({ spec: { title: 'First', version: '1', base_url: 'http://x.test' } }),
      [`GET /openapi/runs/${OTHER_RUN_ID}$`]: () => reportFor({ id: OTHER_RUN_ID, spec: { title: 'Second', version: '2', base_url: 'http://y.test' } }),
    });
    const seen: Array<{ run: string; title: string | null }> = [];
    const { rerender } = renderHook(
      ({ run }) => {
        const look = useOpenApiRun(PROJECT_ID, run);
        seen.push({ run, title: look.report?.spec.title ?? null });
        return look;
      },
      { initialProps: { run: RUN_ID } },
    );
    await settle();
    rerender({ run: OTHER_RUN_ID });
    await settle();
    expect(seen.filter((s) => s.run === OTHER_RUN_ID && s.title === 'First')).toEqual([]);
    expect(seen.find((s) => s.run === OTHER_RUN_ID)?.title).toBeNull();
    expect(seen.at(-1)).toEqual({ run: OTHER_RUN_ID, title: 'Second' });
  });

  it('ignores an answer that arrives after another run was chosen', async () => {
    const late: Array<(r: unknown) => void> = [];
    backend({
      [READ_ROUTE]: () => new Promise((resolve) => late.push(resolve)),
      [`GET /openapi/runs/${OTHER_RUN_ID}$`]: () => reportFor({ spec: { title: 'Second', version: '2', base_url: 'http://y.test' } }),
    });
    const { result, rerender } = renderHook(({ run }) => useOpenApiRun(PROJECT_ID, run), { initialProps: { run: RUN_ID } });
    await settle();
    rerender({ run: OTHER_RUN_ID });
    await settle();
    await act(async () => late[0](reportFor({ spec: { title: 'First', version: '1', base_url: 'http://x.test' } })));
    expect(result.current.report?.spec.title).toBe('Second');
  });

  it('can be asked to read again, and keeps the report while it does', async () => {
    let release: (r: unknown) => void = () => {};
    let reads = 0;
    backend({
      [READ_ROUTE]: () => (++reads === 1 ? reportFor({ final: true, notes: ['before'] }) : new Promise((resolve) => (release = resolve))),
    });
    const { result } = renderHook(() => useOpenApiRun(PROJECT_ID, RUN_ID));
    await settle();
    act(() => result.current.refresh());
    await settle();
    expect(result.current.report?.notes).toEqual(['before']);
    await act(async () => release(reportFor({ final: true, notes: ['after'] })));
    expect(result.current.report?.notes).toEqual(['after']);
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
    const { unmount } = renderHook(() => useOpenApiRun(PROJECT_ID, RUN_ID));
    await settle();
    expect(signal?.aborted).toBe(false);
    unmount();
    expect(signal?.aborted).toBe(true);
  });
});

describe('useOpenApiRuns', () => {
  it('reads the project’s runs, newest first as the service gives them, and says it is loading until they arrive', async () => {
    const fetchMock = backend({ 'GET /openapi/runs$': () => ({ runs: [runRow(), runRow({ id: OTHER_RUN_ID, title: 'Older' })] }) });
    const { result } = renderHook(() => useOpenApiRuns(PROJECT_ID));
    expect(result.current.loading).toBe(true);
    await settle();
    expect(requests(fetchMock, 'GET /openapi/runs$')[0].path).toBe(`/api/v1/projects/${PROJECT_ID}/openapi/runs`);
    expect(result.current.runs?.map((r) => r.title)).toEqual(['Fixture Pets', 'Older']);
    expect(result.current.loading).toBe(false);
    expect(result.current.error).toBeNull();
  });

  it('reads nothing with no project', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useOpenApiRuns(null));
    await settle();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(result.current).toMatchObject({ runs: null, loading: false, error: null });
  });

  it('reads again when asked, and when another project is chosen it forgets the first project’s runs', async () => {
    let reads = 0;
    backend({ 'GET /openapi/runs$': (req) => ({ runs: [runRow({ title: `${req.path.includes('proj-b') ? 'B' : 'A'}${++reads}` })] }) });
    const seen: Array<{ project: string; title: string | null }> = [];
    const { rerender, result } = renderHook(
      ({ project }) => {
        const list = useOpenApiRuns(project);
        seen.push({ project, title: list.runs?.[0]?.title ?? null });
        return list;
      },
      { initialProps: { project: PROJECT_ID } },
    );
    await settle();
    act(() => result.current.refresh());
    await settle();
    expect(result.current.runs?.[0].title).toBe('A2');
    rerender({ project: 'proj-b' });
    await settle();
    expect(seen.filter((s) => s.project === 'proj-b' && s.title?.startsWith('A'))).toEqual([]);
    expect(seen.find((s) => s.project === 'proj-b')?.title).toBeNull();
    expect(result.current.runs?.[0].title).toBe('B3');
  });

  it('keeps the runs it has when reading again fails, and says so; and says so when the first read fails', async () => {
    let reads = 0;
    backend({ 'GET /openapi/runs$': () => (++reads === 2 ? failure(500, 'INTERNAL_ERROR', 'busy') : { runs: [runRow()] }) });
    const { result } = renderHook(() => useOpenApiRuns(PROJECT_ID));
    await settle();
    act(() => result.current.refresh());
    await settle();
    expect(result.current.runs).toHaveLength(1);
    expect(result.current.error?.message).toContain('busy');
    act(() => result.current.refresh());
    await settle();
    expect(result.current.error).toBeNull();

    backend({ 'GET /openapi/runs$': () => failure(403, 'FORBIDDEN', 'not yours') });
    const failing = renderHook(() => useOpenApiRuns('proj-c'));
    await settle();
    expect(failing.result.current.runs).toBeNull();
    expect(failing.result.current.loading).toBe(false);
    expect(failing.result.current.error?.message).toBeTruthy();
  });
});
