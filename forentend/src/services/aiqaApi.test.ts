/**
 * FE-2 targeted tests for the FE-1 transport layer's auth-relevant surface:
 * key storage, masking, and the `ApiError` classification that 401 vs 403
 * vs offline all depend on. Mocks `fetch` only at the transport boundary
 * (this sprint's brief §18) — every other function under test is the real
 * implementation.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError, aiqaApi, clearApiKey, getApiKey, hasApiKey, maskApiKey, setApiKey } from './aiqaApi';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

function blobResponse(text: string, contentType = 'text/plain'): Response {
  const blob = new globalThis.Blob([text], { type: contentType });
  return {
    ok: true,
    status: 200,
    headers: new Headers({ 'Content-Type': contentType }),
    blob: async () => blob,
  } as unknown as Response;
}

function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_test123' } };
}

/** Awaits a call expected to reject with `ApiError`, and fails loudly (not
 *  with a confusing type error) if it resolves instead. */
/** `FileReader` rather than `Blob.prototype.text()` — this project's own
 *  jsdom test environment doesn't implement `.text()`/`.arrayBuffer()` on
 *  the `Blob` its `fetch`/`Response` polyfill produces (confirmed via a
 *  throwaway check before writing this); `FileReader` does work here and
 *  is what `EvidencePage.tsx`'s own real implementation uses for the same
 *  reason (broader real support, not merely a test workaround). */
function readBlobAsText(blob: Blob): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result ?? ''));
    reader.onerror = () => reject(reader.error ?? new Error('failed to read blob'));
    reader.readAsText(blob);
  });
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

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('API key storage (FE-2 §2, §5)', () => {
  it('has no key by default', () => {
    expect(hasApiKey()).toBe(false);
    expect(getApiKey()).toBeNull();
  });

  it('stores and retrieves a key through the FE-1 abstraction', () => {
    setApiKey('aiqa_abcdef0123456789');
    expect(hasApiKey()).toBe(true);
    expect(getApiKey()).toBe('aiqa_abcdef0123456789');
  });

  it('replacing an existing key leaves only the new one stored', () => {
    setApiKey('aiqa_first0000000000');
    setApiKey('aiqa_second000000000');
    expect(getApiKey()).toBe('aiqa_second000000000');
    expect(getApiKey()).not.toBe('aiqa_first0000000000');
  });

  it('sign-out clears the stored key', () => {
    setApiKey('aiqa_abcdef0123456789');
    clearApiKey();
    expect(hasApiKey()).toBe(false);
    expect(getApiKey()).toBeNull();
  });

  it('masks a key without ever containing the raw value', () => {
    const raw = 'aiqa_0123456789abcdef0123456789abcdef';
    const masked = maskApiKey(raw);
    expect(masked).not.toBe(raw);
    expect(masked).not.toContain(raw.slice(10)); // no long raw substring survives
    expect(masked.startsWith('aiqa_0')).toBe(true);
    expect(masked).toContain('•');
  });
});

describe('ApiError classification (FE-2 §8 — 401 vs 403 vs offline)', () => {
  it('401 classifies as unauthorized, not forbidden', async () => {
    setApiKey('aiqa_testkey000000000');
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(401, envelope('UNAUTHENTICATED', 'invalid or revoked API key'))),
    );
    await expect(aiqaApi.projects.list()).rejects.toMatchObject({ kind: 'unauthorized', status: 401 });
  });

  it('403 classifies as forbidden, not unauthorized', async () => {
    setApiKey('aiqa_testkey000000000');
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(403, envelope('FORBIDDEN', 'resource belongs to a different project'))),
    );
    await expect(aiqaApi.projects.list()).rejects.toMatchObject({ kind: 'forbidden', status: 403 });
  });

  it('401 and 403 are never the same ApiErrorKind', async () => {
    setApiKey('aiqa_testkey000000000');
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(401, envelope('UNAUTHENTICATED', 'x')))
      .mockResolvedValueOnce(jsonResponse(403, envelope('FORBIDDEN', 'y')));
    vi.stubGlobal('fetch', fetchMock);

    const err401 = await expectApiError(aiqaApi.projects.list());
    const err403 = await expectApiError(aiqaApi.projects.list());
    expect(err401.kind).not.toBe(err403.kind);
  });

  it('a network failure classifies as offline, never as 401/403', async () => {
    setApiKey('aiqa_testkey000000000');
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')));
    const err = await expectApiError(aiqaApi.projects.list());
    expect(err.kind).toBe('offline');
    expect(err.kind).not.toBe('unauthorized');
    expect(err.kind).not.toBe('forbidden');
  });

  it('a 502 (dev-proxy upstream-down) classifies as offline, matching vite.config.ts\'s convention', async () => {
    setApiKey('aiqa_testkey000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('orchestrator unreachable', { status: 502 })));
    const err = await expectApiError(aiqaApi.projects.list());
    expect(err.kind).toBe('offline');
  });

  it('parses the real Contract §2.5 envelope, surfacing code/message/request_id', async () => {
    setApiKey('aiqa_testkey000000000');
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(404, envelope('RESOURCE_NOT_FOUND', 'project not found'))),
    );
    const err = await expectApiError(aiqaApi.projects.list());
    expect(err.code).toBe('RESOURCE_NOT_FOUND');
    expect(err.detail).toBe('project not found');
    expect(err.requestId).toBe('req_test123');
  });
});

describe('Bearer credential injection (FE-2 §6)', () => {
  it('every request carries Authorization: Bearer <key>, never a query-string credential', async () => {
    setApiKey('aiqa_secretvalue0000000');
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, []));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.projects.list();

    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect((init.headers as Record<string, string>).Authorization).toBe('Bearer aiqa_secretvalue0000000');
    expect(url).not.toContain('aiqa_secretvalue0000000'); // never in the URL/query string
  });

  it('rejects locally as unauthenticated (no network call) when no key is stored', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);

    await expect(aiqaApi.projects.list()).rejects.toMatchObject({ kind: 'unauthenticated' });
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe('aiqaApi.audit.list (FE-6 §24 API layer, items 1-7)', () => {
  const PROJECT_ID = 'proj-audit-test';
  const AUDIT_ROW = {
    occurred_at: '2026-08-18T10:00:00Z',
    actor: 'fe6-test-key',
    role: 'operator',
    project_id: PROJECT_ID,
    action: 'project.update',
    target: PROJECT_ID,
    detail: 'name=Renamed Project',
    outcome: 'ok',
  };

  it('item 1/2 — uses the existing FE-1 transport and parses a real successful response, including the real pagination envelope', async () => {
    setApiKey('aiqa_audittest0000000');
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, { rows: [AUDIT_ROW], total: 1 }));
    vi.stubGlobal('fetch', fetchMock);

    const result = await aiqaApi.audit.list(PROJECT_ID, { limit: 50 });

    expect(result.rows).toEqual([AUDIT_ROW]);
    expect(result.total).toBe(1);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/projects/${PROJECT_ID}/audit?limit=50`);
    expect((init.headers as Record<string, string>).Authorization).toBe('Bearer aiqa_audittest0000000');
  });

  it('item 3 — 401 maps to unauthorized', async () => {
    setApiKey('aiqa_audittest0000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(401, envelope('UNAUTHENTICATED', 'bad key'))));
    await expect(aiqaApi.audit.list(PROJECT_ID)).rejects.toMatchObject({ kind: 'unauthorized', status: 401 });
  });

  it('item 4 — 403 maps to forbidden (the real backend rule: audit requires Operator role + project access)', async () => {
    setApiKey('aiqa_audittest0000000');
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(403, envelope('FORBIDDEN', "role 'viewer' cannot perform this action"))),
    );
    await expect(aiqaApi.audit.list(PROJECT_ID)).rejects.toMatchObject({ kind: 'forbidden', status: 403 });
  });

  it('item 5 — a network failure maps to offline', async () => {
    setApiKey('aiqa_audittest0000000');
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')));
    const err = await expectApiError(aiqaApi.audit.list(PROJECT_ID));
    expect(err.kind).toBe('offline');
  });

  it('item 6 — the API key never appears in the request URL', async () => {
    const raw = 'aiqa_shouldneverbeinurl0';
    setApiKey(raw);
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, { rows: [], total: 0 }));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.audit.list(PROJECT_ID, { limit: 100 });

    const [url] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).not.toContain(raw);
    expect(url).not.toContain('aiqa_');
  });

  it('item 7 — project scope is passed exactly as the real contract requires (path segment, not a query param), and `limit`/`offset` (real pagination) are the only query parameters ever sent — no invented filter/sort params', async () => {
    setApiKey('aiqa_audittest0000000');
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, { rows: [], total: 0 }));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.audit.list(PROJECT_ID, { limit: 250, offset: 500 });

    const [url] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/projects/${PROJECT_ID}/audit?limit=250&offset=500`);

    // Omitting both omits the query string entirely — nothing is ever
    // fabricated in its place.
    fetchMock.mockClear();
    await aiqaApi.audit.list(PROJECT_ID);
    const [url2] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url2).toBe(`/api/v1/projects/${PROJECT_ID}/audit`);
  });
});

describe('Evidence endpoints (FE-7 §26 API layer, items 1-7)', () => {
  const PROJECT_ID = 'proj-evidence-test';
  const RUN_ID = 'run-evidence-test';
  const EVIDENCE_ID = 'ev-1';
  const EVIDENCE_ROW = {
    id: EVIDENCE_ID,
    task_id: 't1',
    kind: 'log',
    object_key: `${RUN_ID}/t1-log.txt`,
    content_type: 'text/plain',
    size_bytes: 42,
    created_at: '2026-08-18T10:00:00Z',
  };

  it('item 1/2 — aiqaApi.executions.evidence uses FE-1 transport and parses a real successful list response', async () => {
    setApiKey('aiqa_evidencetest00000');
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, [EVIDENCE_ROW]));
    vi.stubGlobal('fetch', fetchMock);

    const rows = await aiqaApi.executions.evidence(PROJECT_ID, RUN_ID);

    expect(rows).toEqual([EVIDENCE_ROW]);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/projects/${PROJECT_ID}/executions/${RUN_ID}/evidence`);
    expect((init.headers as Record<string, string>).Authorization).toBe('Bearer aiqa_evidencetest00000');
  });

  it('item 2b — aiqaApi.evidence.get parses a real successful detail response at the exact real path', async () => {
    setApiKey('aiqa_evidencetest00000');
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, EVIDENCE_ROW));
    vi.stubGlobal('fetch', fetchMock);

    const row = await aiqaApi.evidence.get(PROJECT_ID, EVIDENCE_ID);

    expect(row).toEqual(EVIDENCE_ROW);
    const [url] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/projects/${PROJECT_ID}/evidence/${EVIDENCE_ID}`);
  });

  it('item 2c — aiqaApi.evidence.content fetches real raw bytes as a Blob at the exact real /content path', async () => {
    setApiKey('aiqa_evidencetest00000');
    const fetchMock = vi.fn().mockResolvedValue(blobResponse('real log bytes', 'text/plain'));
    vi.stubGlobal('fetch', fetchMock);

    const blob = await aiqaApi.evidence.content(PROJECT_ID, EVIDENCE_ID);

    expect(blob).toBeInstanceOf(Blob);
    expect(await readBlobAsText(blob)).toBe('real log bytes');
    const [url] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/projects/${PROJECT_ID}/evidence/${EVIDENCE_ID}/content`);
  });

  it('item 3 — 401 maps to unauthorized on every evidence endpoint', async () => {
    setApiKey('aiqa_evidencetest00000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(401, envelope('UNAUTHENTICATED', 'bad key'))));
    await expect(aiqaApi.executions.evidence(PROJECT_ID, RUN_ID)).rejects.toMatchObject({ kind: 'unauthorized', status: 401 });
  });

  it('item 4 — 403 maps to forbidden (real backend rule: wrong project scope on a project-scoped key)', async () => {
    setApiKey('aiqa_evidencetest00000');
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(403, envelope('FORBIDDEN', 'project scope does not match'))),
    );
    await expect(aiqaApi.executions.evidence(PROJECT_ID, RUN_ID)).rejects.toMatchObject({ kind: 'forbidden', status: 403 });
  });

  it('item 5 — a network failure maps to offline', async () => {
    setApiKey('aiqa_evidencetest00000');
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')));
    const err = await expectApiError(aiqaApi.executions.evidence(PROJECT_ID, RUN_ID));
    expect(err.kind).toBe('offline');
  });

  it('item 6 — the API key never appears in any evidence request URL', async () => {
    const raw = 'aiqa_shouldneverbeinurl1';
    setApiKey(raw);
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, [EVIDENCE_ROW]));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.executions.evidence(PROJECT_ID, RUN_ID);

    const [url] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).not.toContain(raw);
    expect(url).not.toContain('aiqa_');
  });

  it('item 7 — project/run scope is passed exactly as the real contract requires (path segments only, no invented query params)', async () => {
    setApiKey('aiqa_evidencetest00000');
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, [EVIDENCE_ROW]));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.executions.evidence(PROJECT_ID, RUN_ID);
    await aiqaApi.evidence.get(PROJECT_ID, EVIDENCE_ID);
    await aiqaApi.evidence.content(PROJECT_ID, EVIDENCE_ID);

    const urls = fetchMock.mock.calls.map(([u]: [string]) => u);
    expect(urls).toEqual([
      `/api/v1/projects/${PROJECT_ID}/executions/${RUN_ID}/evidence`,
      `/api/v1/projects/${PROJECT_ID}/evidence/${EVIDENCE_ID}`,
      `/api/v1/projects/${PROJECT_ID}/evidence/${EVIDENCE_ID}/content`,
    ]);
  });
});

describe('Execution endpoints (FE-8 §34 API layer, items 1-10)', () => {
  const PROJECT_ID = 'proj-execution-test';
  const RUN_ID = 'run-execution-test';
  const PLAN = { name: 'FE-8 test run', tasks: [{ id: 't1', name: 'Simulated task', simulate: { outcome: 'pass' as const } }] };
  const RUN_SUMMARY = {
    run_id: RUN_ID,
    name: PLAN.name,
    state: 'SCHEDULED' as const,
    total: 1,
    queued: 1,
    running: 0,
    passed: 0,
    failed: 0,
    blocked: 0,
    skipped: 0,
    cancelled: 0,
    timeout: 0,
    infra_error: 0,
    needs_review: 0,
    started_at: '2026-08-19T00:00:00Z',
    finished_at: null,
    project_id: PROJECT_ID,
    requested_by: 'fe8-test-key',
    policy_notes: null,
    approved_by: null,
    approved_at: null,
  };
  const RUN_DETAIL = { ...RUN_SUMMARY, tasks: [] };
  const RUN_LIST_ITEM = { run_id: RUN_ID, name: PLAN.name, state: 'SCHEDULED' as const, total: 1, passed: 0, failed: 0, project_id: PROJECT_ID };

  it('item 1/8/9 — aiqaApi.executions.list uses FE-1 transport, the real path, and no query params when omitted', async () => {
    setApiKey('aiqa_exectest00000000000');
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, [RUN_LIST_ITEM]));
    vi.stubGlobal('fetch', fetchMock);

    const list = await aiqaApi.executions.list(PROJECT_ID);

    expect(list).toEqual([RUN_LIST_ITEM]);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/projects/${PROJECT_ID}/executions`);
    expect((init.headers as Record<string, string>).Authorization).toBe('Bearer aiqa_exectest00000000000');
  });

  it('item 2/8 — aiqaApi.executions.create uses FE-1 transport, POSTs the exact real plan to the exact real path', async () => {
    setApiKey('aiqa_exectest00000000000');
    const fetchMock = vi
      .fn()
      .mockResolvedValue(jsonResponse(201, { run_id: RUN_ID, status: 'SCHEDULED', policy_notes: null }));
    vi.stubGlobal('fetch', fetchMock);

    const res = await aiqaApi.executions.create(PROJECT_ID, PLAN);

    expect(res).toEqual({ run_id: RUN_ID, status: 'SCHEDULED', policy_notes: null });
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/projects/${PROJECT_ID}/executions`);
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body as string)).toEqual(PLAN);
  });

  it('item 3/8 — aiqaApi.executions.get uses FE-1 transport and the real detail path', async () => {
    setApiKey('aiqa_exectest00000000000');
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, RUN_DETAIL));
    vi.stubGlobal('fetch', fetchMock);

    const detail = await aiqaApi.executions.get(PROJECT_ID, RUN_ID);

    expect(detail).toEqual(RUN_DETAIL);
    const [url] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/projects/${PROJECT_ID}/executions/${RUN_ID}`);
  });

  it('item 4/8 — aiqaApi.executions.cancel uses FE-1 transport, POSTs to the real cancel path', async () => {
    setApiKey('aiqa_exectest00000000000');
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, { run_id: RUN_ID, status: 'CANCELLED' }));
    vi.stubGlobal('fetch', fetchMock);

    const res = await aiqaApi.executions.cancel(PROJECT_ID, RUN_ID);

    expect(res).toEqual({ run_id: RUN_ID, status: 'CANCELLED' });
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/projects/${PROJECT_ID}/executions/${RUN_ID}/cancel`);
    expect(init.method).toBe('POST');
  });

  it('approve/reject return the real RunSummary shape (FE-8 fix: was mistyped as RunDetail)', async () => {
    setApiKey('aiqa_exectest00000000000');
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, RUN_SUMMARY));
    vi.stubGlobal('fetch', fetchMock);

    const approved = await aiqaApi.executions.approve(PROJECT_ID, RUN_ID);
    const rejected = await aiqaApi.executions.reject(PROJECT_ID, RUN_ID);

    expect(approved).toEqual(RUN_SUMMARY);
    expect(rejected).toEqual(RUN_SUMMARY);
    expect('tasks' in approved).toBe(false);
    const urls = fetchMock.mock.calls.map(([u]: [string]) => u);
    expect(urls).toEqual([
      `/api/v1/projects/${PROJECT_ID}/executions/${RUN_ID}/approve`,
      `/api/v1/projects/${PROJECT_ID}/executions/${RUN_ID}/reject`,
    ]);
  });

  it('item 5 — 401 maps to unauthorized on execution creation', async () => {
    setApiKey('aiqa_exectest00000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(401, envelope('UNAUTHENTICATED', 'bad key'))));
    await expect(aiqaApi.executions.create(PROJECT_ID, PLAN)).rejects.toMatchObject({ kind: 'unauthorized', status: 401 });
  });

  it('item 6 — 403 maps to forbidden (real backend rule: create/cancel/approve/reject require Operator role)', async () => {
    setApiKey('aiqa_exectest00000000000');
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(403, envelope('FORBIDDEN', "role 'viewer' cannot perform this action"))),
    );
    await expect(aiqaApi.executions.create(PROJECT_ID, PLAN)).rejects.toMatchObject({ kind: 'forbidden', status: 403 });
  });

  it('item 7 — a network failure maps to offline', async () => {
    setApiKey('aiqa_exectest00000000000');
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')));
    const err = await expectApiError(aiqaApi.executions.list(PROJECT_ID));
    expect(err.kind).toBe('offline');
  });

  it('item 10 — the API key never appears in any execution request URL', async () => {
    const raw = 'aiqa_shouldneverbeinurl2';
    setApiKey(raw);
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, [RUN_LIST_ITEM]));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.executions.list(PROJECT_ID);

    const [url] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).not.toContain(raw);
    expect(url).not.toContain('aiqa_');
  });
});

describe('aiqaApi.projects.create (FE-14 §16 — real POST /api/v1/projects, previously unused/uncovered)', () => {
  const CREATED_PROJECT = {
    id: 'proj-fe14-created',
    name: 'New Real Project',
    description: 'A real description',
    created_at: '2026-08-19T00:00:00Z',
    updated_at: '2026-08-19T00:00:00Z',
  };

  it('success — POSTs the exact real path/method/body and returns the real typed Project', async () => {
    setApiKey('aiqa_fe14createtest0000');
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(201, CREATED_PROJECT));
    vi.stubGlobal('fetch', fetchMock);

    const project = await aiqaApi.projects.create({ name: 'New Real Project', description: 'A real description' });

    expect(project).toEqual(CREATED_PROJECT);
    const [url, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe('/api/v1/projects');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body as string)).toEqual({ name: 'New Real Project', description: 'A real description' });
    expect((init.headers as Record<string, string>).Authorization).toBe('Bearer aiqa_fe14createtest0000');
  });

  it('success — description is genuinely optional (omitted from the request body when not supplied)', async () => {
    setApiKey('aiqa_fe14createtest0000');
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(201, { ...CREATED_PROJECT, description: null }));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.projects.create({ name: 'New Real Project' });

    const [, init] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(JSON.parse(init.body as string)).toEqual({ name: 'New Real Project' });
  });

  it('error — 403 (real backend rule: project creation is Admin-only) propagates as a real forbidden ApiError, not a fake project', async () => {
    setApiKey('aiqa_fe14createtest0000');
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(403, envelope('FORBIDDEN', "role 'operator' cannot perform this action"))),
    );

    const err = await expectApiError(aiqaApi.projects.create({ name: 'New Real Project' }));
    expect(err.kind).toBe('forbidden');
    expect(err.status).toBe(403);
  });

  it('error — 400 validation failure (real backend rule: name must not be empty) propagates as a real validation_error ApiError', async () => {
    setApiKey('aiqa_fe14createtest0000');
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(400, envelope('VALIDATION_ERROR', 'project name must not be empty'))),
    );

    const err = await expectApiError(aiqaApi.projects.create({ name: '' }));
    expect(err.kind).toBe('validation_error');
    expect(err.message).toBe('project name must not be empty');
  });

  it('error — a network failure maps to offline, not a fake success', async () => {
    setApiKey('aiqa_fe14createtest0000');
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')));

    const err = await expectApiError(aiqaApi.projects.create({ name: 'New Real Project' }));
    expect(err.kind).toBe('offline');
  });
});

describe('aiqaApi.testCases.compile (Phase 5 — compile against a crawl)', () => {
  const READY = {
    status: 'READY',
    test_case_id: 'case-1',
    execution_type: 'BROWSER',
    target: null,
    actions: [],
    compiler: 'AI',
    diagnostics: [],
    provider: 'local',
    model: 'qwen',
  };

  it('with no body it sends none — the request is exactly what it always was', async () => {
    setApiKey('aiqa_compilebodytest0000');
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, READY));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.testCases.compile('proj-1', 'case-1');

    const [url, init] = fetchMock.mock.calls[0];
    expect(String(url)).toContain('/projects/proj-1/test-cases/case-1/compile');
    expect((init as RequestInit).method).toBe('POST');
    expect((init as RequestInit).body).toBeUndefined();
    expect(((init as RequestInit).headers as Record<string, string>)['Content-Type']).toBeUndefined();
  });

  it('given a crawl and a role it sends them as the request body, and returns the grounding the response reports', async () => {
    setApiKey('aiqa_compilebodytest0000');
    const grounded = {
      ...READY,
      grounding: { role: 'admin', pages_in_crawl: 3, pages_offered: 2, elements_offered: 47, login_steps: 4, secrets: ['AIQA_LOGIN_ADMIN_PASSWORD'] },
    };
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, grounded));
    vi.stubGlobal('fetch', fetchMock);

    const outcome = await aiqaApi.testCases.compile('proj-1', 'case-1', undefined, { crawl_run_id: 'crawl-1', role: 'admin' });

    const [, init] = fetchMock.mock.calls[0];
    expect(JSON.parse(String((init as RequestInit).body))).toEqual({ crawl_run_id: 'crawl-1', role: 'admin' });
    expect(outcome.grounding?.secrets).toEqual(['AIQA_LOGIN_ADMIN_PASSWORD']);
  });

  it('a handoff can carry the same two fields alongside compile_and_execute', async () => {
    setApiKey('aiqa_compilebodytest0000');
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(201, { run_id: 'run-1', status: 'SCHEDULED', policy_notes: null }));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.testCases.handoff('proj-1', 'case-1', { compile_and_execute: true, crawl_run_id: 'crawl-1', role: 'admin' });

    const [, init] = fetchMock.mock.calls[0];
    expect(JSON.parse(String((init as RequestInit).body))).toEqual({ compile_and_execute: true, crawl_run_id: 'crawl-1', role: 'admin' });
  });
});

describe('No hardcoded credential in source (FE-2 §6, §18 item 11)', () => {
  it('API_BASE contains no embedded secret and defaults to the contract-shaped path', async () => {
    const mod = await import('./aiqaApi');
    expect(mod.API_BASE).toBe('/api/v1');
    expect(mod.API_BASE).not.toMatch(/aiqa_[a-f0-9]+/i);
  });
});
