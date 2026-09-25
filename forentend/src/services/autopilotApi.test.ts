/**
 * Phase 7 — the autopilot's part of the API client: what each call sends and how a refusal reads. `fetch` is mocked at the
 * transport boundary only; everything else is the real client.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError, aiqaApi, setApiKey } from './aiqaApi';

const PROJECT = 'proj-1';
const RUN = 'run-1';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

function textResponse(status: number, body: string): Response {
  return new Response(body, { status, headers: { 'Content-Type': 'text/markdown; charset=utf-8' } });
}

function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_test' } };
}

async function expectApiError(promise: Promise<unknown>): Promise<ApiError> {
  try {
    await promise;
  } catch (e) {
    if (e instanceof ApiError) return e;
    throw e;
  }
  throw new Error('expected the promise to reject with an ApiError, but it resolved');
}

/** The one call the mock saw: its URL parts, method, headers and body. */
function seen(fetchMock: ReturnType<typeof vi.fn>, at = 0) {
  const [url, init] = fetchMock.mock.calls[at] as [string, RequestInit];
  const parsed = new URL(url, 'http://localhost');
  return {
    path: parsed.pathname,
    query: Object.fromEntries(parsed.searchParams.entries()),
    method: init.method,
    headers: (init.headers ?? {}) as Record<string, string>,
    body: init.body === undefined ? undefined : JSON.parse(String(init.body)),
    raw: url,
  };
}

const RUN_DETAIL = { run: { id: RUN, state: 'AWAITING_APPROVAL' }, steps: [], events: [], working: false };

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
  setApiKey('aiqa_autopilottest0000');
});

afterEach(() => {
  localStorage.clear();
});

describe('autopilot client — what each call sends', () => {
  it('estimate posts the request to the project’s estimate route, and an empty request when none is given', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, { requirements: 6 }));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.autopilot.estimate(PROJECT, { depth: 'STANDARD', roles: 2, run_concurrency: 4 });
    await aiqaApi.autopilot.estimate(PROJECT);

    const first = seen(fetchMock, 0);
    expect(first.path).toBe(`/api/v1/projects/${PROJECT}/autopilot/estimate`);
    expect(first.method).toBe('POST');
    expect(first.body).toEqual({ depth: 'STANDARD', roles: 2, run_concurrency: 4 });
    expect(seen(fetchMock, 1).body).toEqual({});
  });

  it('runs.create posts the config as it is given and returns the run with its steps as sent', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(201, RUN_DETAIL));
    vi.stubGlobal('fetch', fetchMock);
    const config = { base_url: 'https://shop.example.test', roles: [{ name: 'anonymous', login_actions: [] }], execute: false };

    const made = await aiqaApi.autopilot.runs.create(PROJECT, config);

    const call = seen(fetchMock);
    expect(call.path).toBe(`/api/v1/projects/${PROJECT}/autopilot/runs`);
    expect(call.method).toBe('POST');
    expect(call.body).toEqual(config);
    expect(made).toEqual(RUN_DETAIL);
  });

  it('list and get read the project’s runs and one run with GET and no body', async () => {
    const fetchMock = vi.fn().mockResolvedValueOnce(jsonResponse(200, { runs: [] })).mockResolvedValueOnce(jsonResponse(200, RUN_DETAIL));
    vi.stubGlobal('fetch', fetchMock);

    const list = await aiqaApi.autopilot.runs.list(PROJECT);
    const one = await aiqaApi.autopilot.runs.get(PROJECT, RUN);

    expect(list).toEqual({ runs: [] });
    expect(one).toEqual(RUN_DETAIL);
    expect(seen(fetchMock, 0)).toMatchObject({ path: `/api/v1/projects/${PROJECT}/autopilot/runs`, method: 'GET', body: undefined });
    expect(seen(fetchMock, 1)).toMatchObject({ path: `/api/v1/projects/${PROJECT}/autopilot/runs/${RUN}`, method: 'GET', body: undefined });
  });

  it('start sends what the person accepted, and only that', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, RUN_DETAIL));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.autopilot.runs.start(PROJECT, RUN, { accept_estimate: true, accept_paid: true });
    await aiqaApi.autopilot.runs.start(PROJECT, RUN, { accept_estimate: true });

    expect(seen(fetchMock, 0)).toMatchObject({ path: `/api/v1/projects/${PROJECT}/autopilot/runs/${RUN}/start`, method: 'POST', body: { accept_estimate: true, accept_paid: true } });
    expect(seen(fetchMock, 1).body).toEqual({ accept_estimate: true });
  });

  it('cancel, continue and resume are POSTs with no body to their own routes', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, { ...RUN_DETAIL, outcome: 'requested' }));
    vi.stubGlobal('fetch', fetchMock);

    const cancelled = await aiqaApi.autopilot.runs.cancel(PROJECT, RUN);
    await aiqaApi.autopilot.runs.continue(PROJECT, RUN);
    await aiqaApi.autopilot.runs.resume(PROJECT, RUN);

    expect(cancelled.outcome).toBe('requested');
    const tails = [0, 1, 2].map((i) => seen(fetchMock, i));
    expect(tails.map((c) => c.path.split('/').pop())).toEqual(['cancel', 'continue', 'resume']);
    for (const call of tails) {
      expect(call.method).toBe('POST');
      expect(call.body).toBeUndefined();
      expect(call.headers['Content-Type']).toBeUndefined();
    }
  });

  it('approveCases asks for every validated case with an empty body, or for the named ones', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, { suite_id: 's', approved: [], refused: [] }));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.autopilot.runs.approveCases(PROJECT, RUN);
    await aiqaApi.autopilot.runs.approveCases(PROJECT, RUN, []);
    await aiqaApi.autopilot.runs.approveCases(PROJECT, RUN, ['c1', 'c2']);

    expect(seen(fetchMock, 0).path).toBe(`/api/v1/projects/${PROJECT}/autopilot/runs/${RUN}/approve-cases`);
    expect(seen(fetchMock, 0).body).toEqual({});
    expect(seen(fetchMock, 1).body).toEqual({});
    expect(seen(fetchMock, 2).body).toEqual({ case_ids: ['c1', 'c2'] });
  });

  it('events puts after and limit in the query only when they are given', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, { events: [] }));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.autopilot.runs.events(PROJECT, RUN);
    await aiqaApi.autopilot.runs.events(PROJECT, RUN, { after: 12, limit: 50 });
    await aiqaApi.autopilot.runs.events(PROJECT, RUN, { after: 0 });

    expect(seen(fetchMock, 0).query).toEqual({});
    expect(seen(fetchMock, 1).query).toEqual({ after: '12', limit: '50' });
    expect(seen(fetchMock, 2).query, 'an id of 0 is an id').toEqual({ after: '0' });
    expect(seen(fetchMock, 0).path).toBe(`/api/v1/projects/${PROJECT}/autopilot/runs/${RUN}/events`);
  });

  it('report reads the JSON report, and reportMarkdown reads the same route as text', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(200, { final: true }))
      .mockResolvedValueOnce(textResponse(200, '# Autopilot report: Release 5\n'));
    vi.stubGlobal('fetch', fetchMock);

    const report = await aiqaApi.autopilot.runs.report(PROJECT, RUN);
    const markdown = await aiqaApi.autopilot.runs.reportMarkdown(PROJECT, RUN);

    expect(report).toEqual({ final: true });
    expect(markdown).toBe('# Autopilot report: Release 5\n');
    expect(seen(fetchMock, 0)).toMatchObject({ path: `/api/v1/projects/${PROJECT}/autopilot/runs/${RUN}/report`, query: {} });
    expect(seen(fetchMock, 1)).toMatchObject({ path: `/api/v1/projects/${PROJECT}/autopilot/runs/${RUN}/report`, query: { format: 'markdown' } });
  });

  it('loginSuggestion posts the role, the crawl and whether the AI may be asked', async () => {
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, { crawl_run_id: 'c', suggestion: {}, alternatives: [], ai: { requested: true, used: false, note: null }, missing_secrets: [] }));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.loginSuggestion.suggest(PROJECT, { role: 'admin', crawl_run_id: 'crawl-9', use_ai: true });

    expect(seen(fetchMock)).toMatchObject({ path: `/api/v1/projects/${PROJECT}/login-suggestion`, method: 'POST', body: { role: 'admin', crawl_run_id: 'crawl-9', use_ai: true } });
  });

  it('never puts the API key in a URL', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, {}));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.autopilot.runs.get(PROJECT, RUN);
    await aiqaApi.autopilot.runs.events(PROJECT, RUN, { after: 1 });
    await aiqaApi.autopilot.runs.reportMarkdown(PROJECT, RUN);

    for (let i = 0; i < 3; i++) expect(seen(fetchMock, i).raw).not.toContain('aiqa_autopilottest0000');
  });
});

describe('autopilot client — how a refusal reads', () => {
  const respond = (status: number, code: string, message: string) =>
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(status, envelope(code, message))));

  it('a run refused for what could only fail later is a validation error carrying the reason', async () => {
    respond(400, 'VALIDATION_ERROR', 'environment variable AIQA_LOGIN_ADMIN_PASSWORD is not set');
    const refused = await expectApiError(aiqaApi.autopilot.runs.create(PROJECT, { base_url: 'x', roles: [] }));
    expect([refused.kind, refused.status]).toEqual(['validation_error', 400]);
    expect(refused.detail).toContain('AIQA_LOGIN_ADMIN_PASSWORD');
  });

  it('a second run while one is going, and starting a run twice, are conflicts', async () => {
    respond(409, 'CONFLICT', 'this project already has an autopilot run that has not ended');
    const second = await expectApiError(aiqaApi.autopilot.runs.create(PROJECT, { base_url: 'x', roles: [] }));
    expect([second.kind, second.status]).toEqual(['conflict', 409]);
    respond(409, 'CONFLICT', 'this run is RUNNING, not awaiting approval');
    const twice = await expectApiError(aiqaApi.autopilot.runs.start(PROJECT, RUN, { accept_estimate: true }));
    expect(twice.kind).toBe('conflict');
  });

  it('starting without accepting the estimate, or a paid one without the cost, is refused as validation', async () => {
    respond(400, 'VALIDATION_ERROR', 'this run would call a provider that charges: accept_paid: true says you accept that cost');
    const refused = await expectApiError(aiqaApi.autopilot.runs.start(PROJECT, RUN, { accept_estimate: true }));
    expect(refused.kind).toBe('validation_error');
    expect(refused.detail).toContain('accept_paid');
  });

  it('a viewer, and a run of another project, are told apart', async () => {
    respond(403, 'FORBIDDEN', "role 'viewer' cannot perform this action");
    expect((await expectApiError(aiqaApi.autopilot.runs.cancel(PROJECT, RUN))).kind).toBe('forbidden');
    respond(404, 'RESOURCE_NOT_FOUND', 'autopilot run not found in this project');
    expect((await expectApiError(aiqaApi.autopilot.runs.get(PROJECT, RUN))).kind).toBe('not_found');
  });
});
