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

describe('No hardcoded credential in source (FE-2 §6, §18 item 11)', () => {
  it('API_BASE contains no embedded secret and defaults to the contract-shaped path', async () => {
    const mod = await import('./aiqaApi');
    expect(mod.API_BASE).toBe('/api/v1');
    expect(mod.API_BASE).not.toMatch(/aiqa_[a-f0-9]+/i);
  });
});
