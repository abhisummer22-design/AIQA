/**
 * FE-2 targeted tests for the auth state abstraction itself — the state
 * transitions the brief's minimum test list (§18) asks for, exercised
 * through the real `AuthProvider`/`useAuth`, with `fetch` mocked only at
 * the transport boundary.
 */
import React from 'react';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AuthProvider, useAuth } from './AuthContext';
import { getApiKey, setApiKey } from '../services/aiqaApi';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_test' } };
}

const wrapper = ({ children }: { children: React.ReactNode }) => <AuthProvider>{children}</AuthProvider>;

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('AuthContext (FE-2 §7 state machine)', () => {
  it('item 1 — no key configured yields the unauthenticated/no-key state, not a false "authenticated"', () => {
    const { result } = renderHook(() => useAuth(), { wrapper });
    expect(result.current.status).toBe('no_key');
    expect(result.current.hasKey).toBe(false);
    expect(result.current.maskedKey).toBeNull();
  });

  it('item 2 — saving a key stores it through the FE-1 aiqaApi abstraction', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    const { result } = renderHook(() => useAuth(), { wrapper });

    await act(async () => {
      await result.current.saveKey('aiqa_realkey000000000');
    });

    expect(getApiKey()).toBe('aiqa_realkey000000000');
    expect(result.current.hasKey).toBe(true);
  });

  it('item 3 — a stored key is exposed only in masked form', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    const { result } = renderHook(() => useAuth(), { wrapper });

    await act(async () => {
      await result.current.saveKey('aiqa_0123456789abcdef');
    });

    expect(result.current.maskedKey).toMatch(/^aiqa_0.*•+$/);
    expect(result.current.maskedKey).not.toBe('aiqa_0123456789abcdef');
  });

  it('item 4 — the raw key never appears anywhere in the context value after save', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    const { result } = renderHook(() => useAuth(), { wrapper });
    const raw = 'aiqa_supersecretraw000';

    await act(async () => {
      await result.current.saveKey(raw);
    });

    const serialized = JSON.stringify({
      status: result.current.status,
      hasKey: result.current.hasKey,
      maskedKey: result.current.maskedKey,
      lastError: result.current.lastError,
    });
    expect(serialized).not.toContain(raw);
  });

  it('item 5 — sign-out clears the key and returns to the unauthenticated/no-key state', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    const { result } = renderHook(() => useAuth(), { wrapper });

    await act(async () => {
      await result.current.saveKey('aiqa_realkey000000000');
    });
    expect(result.current.hasKey).toBe(true);

    act(() => {
      result.current.signOut();
    });

    expect(result.current.status).toBe('no_key');
    expect(result.current.hasKey).toBe(false);
    expect(result.current.maskedKey).toBeNull();
    expect(getApiKey()).toBeNull();
  });

  it('item 6 — saving a new key replaces the old one, never keeping both', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    const { result } = renderHook(() => useAuth(), { wrapper });

    await act(async () => {
      await result.current.saveKey('aiqa_firstkey00000000');
    });
    await act(async () => {
      await result.current.saveKey('aiqa_secondkey0000000');
    });

    expect(getApiKey()).toBe('aiqa_secondkey0000000');
    expect(result.current.maskedKey).not.toContain('firstkey');
  });

  it('item 7 — a 401 from validation surfaces the unauthenticated state, key stays stored', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(401, envelope('UNAUTHENTICATED', 'invalid or revoked API key'))),
    );
    const { result } = renderHook(() => useAuth(), { wrapper });

    await act(async () => {
      await result.current.saveKey('aiqa_badkey00000000000');
    });

    expect(result.current.status).toBe('unauthenticated');
    expect(result.current.lastError?.kind).toBe('unauthorized');
    // Brief §12: do not automatically clear the key on 401.
    expect(getApiKey()).toBe('aiqa_badkey00000000000');
  });

  it('item 8 — a 403 from validation surfaces the forbidden state, distinct from 401, key stays stored', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(jsonResponse(403, envelope('FORBIDDEN', 'resource belongs to a different project'))),
    );
    const { result } = renderHook(() => useAuth(), { wrapper });

    await act(async () => {
      await result.current.saveKey('aiqa_scopedkey00000000');
    });

    expect(result.current.status).toBe('forbidden');
    expect(result.current.status).not.toBe('unauthenticated');
    expect(getApiKey()).toBe('aiqa_scopedkey00000000');
  });

  it('item 9 — a network/offline failure keeps its own classification, never collapsing into 401/403', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')));
    const { result } = renderHook(() => useAuth(), { wrapper });

    await act(async () => {
      await result.current.saveKey('aiqa_anykey0000000000');
    });

    expect(result.current.status).toBe('offline');
    expect(result.current.status).not.toBe('unauthenticated');
    expect(result.current.status).not.toBe('forbidden');
  });

  it('validate() is a no-op (stays no_key) when nothing is stored — no request is made', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useAuth(), { wrapper });

    await act(async () => {
      await result.current.validate();
    });

    expect(result.current.status).toBe('no_key');
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('a successful validation transitions key_present -> checking -> authenticated', async () => {
    // Seeds storage directly via the real `setApiKey`, simulating a key
    // saved in a previous session, then mounts a *fresh* provider (as a
    // page refresh would) rather than going through `saveKey` in this
    // render.
    setApiKey('aiqa_preexisting00000');
    let resolveFetch: (r: Response) => void;
    vi.stubGlobal(
      'fetch',
      vi.fn().mockReturnValue(
        new Promise<Response>((resolve) => {
          resolveFetch = resolve;
        }),
      ),
    );

    const { result } = renderHook(() => useAuth(), { wrapper });
    expect(result.current.status).toBe('key_present');

    let validatePromise!: Promise<void>;
    act(() => {
      validatePromise = result.current.validate();
    });
    await waitFor(() => expect(result.current.status).toBe('checking'));

    await act(async () => {
      resolveFetch!(jsonResponse(200, []));
      await validatePromise;
    });

    expect(result.current.status).toBe('authenticated');
  });
});
