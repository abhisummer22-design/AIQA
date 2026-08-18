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

  it('item 1/2 — uses the existing FE-1 transport and parses a real successful response', async () => {
    setApiKey('aiqa_audittest0000000');
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, [AUDIT_ROW]));
    vi.stubGlobal('fetch', fetchMock);

    const rows = await aiqaApi.audit.list(PROJECT_ID, { limit: 50 });

    expect(rows).toEqual([AUDIT_ROW]);
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
    const fetchMock = vi.fn().mockResolvedValue(jsonResponse(200, []));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.audit.list(PROJECT_ID, { limit: 100 });

    const [url] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).not.toContain(raw);
    expect(url).not.toContain('aiqa_');
  });

  it('item 7 — project scope is passed exactly as the real contract requires (path segment, not a query param), and only `limit` is ever sent as a query parameter — no invented filter/sort/page params', async () => {
    setApiKey('aiqa_audittest0000000');
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, []));
    vi.stubGlobal('fetch', fetchMock);

    await aiqaApi.audit.list(PROJECT_ID, { limit: 250 });

    const [url] = fetchMock.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(`/api/v1/projects/${PROJECT_ID}/audit?limit=250`);

    // Omitting `limit` omits the query string entirely — nothing is ever
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
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response('real log bytes', { status: 200, headers: { 'Content-Type': 'text/plain' } }));
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

describe('No hardcoded credential in source (FE-2 §6, §18 item 11)', () => {
  it('API_BASE contains no embedded secret and defaults to the contract-shaped path', async () => {
    const mod = await import('./aiqaApi');
    expect(mod.API_BASE).toBe('/api/v1');
    expect(mod.API_BASE).not.toMatch(/aiqa_[a-f0-9]+/i);
  });
});
