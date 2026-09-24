import { renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setApiKey } from '../services/aiqaApi';
import { useProviderNames } from './useProviderNames';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const STATUS = {
  providers: [
    { id: 'ollama', name: 'Ollama' },
    { id: 'custom-ai-3f2a', name: 'My Paid API' },
  ],
  settings: { mode: 'AUTO', preferred_provider: null, preferred_model: null, fallback_enabled: true },
  routing_preview: null,
};

beforeEach(() => {
  localStorage.clear();
  setApiKey('aiqa_providernames000000000000');
});

afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
});

describe('useProviderNames', () => {
  it('turns a provider id into the name it was given once the names are read', async () => {
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, STATUS)));

    const { result } = renderHook(() => useProviderNames(true));

    expect(result.current('custom-ai-3f2a')).toBe('custom-ai-3f2a'); // shown as is until the names arrive
    await waitFor(() => expect(result.current('custom-ai-3f2a')).toBe('My Paid API'));
    expect(result.current('ollama')).toBe('Ollama');
    expect(result.current('a-provider-nobody-knows')).toBe('a-provider-nobody-knows');
  });

  it('reads nothing while there is no result to label', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, STATUS));
    vi.stubGlobal('fetch', fetchMock);

    const { result, rerender } = renderHook(({ wanted }) => useProviderNames(wanted), { initialProps: { wanted: false } });
    await Promise.resolve();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(result.current('custom-ai-3f2a')).toBe('custom-ai-3f2a');

    rerender({ wanted: true });
    await waitFor(() => expect(result.current('custom-ai-3f2a')).toBe('My Paid API'));
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('keeps showing ids when the names cannot be read, without throwing', async () => {
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(500, { error: { code: 'INTERNAL_ERROR', message: 'boom', details: null, request_id: 'r1' } })));

    const { result } = renderHook(() => useProviderNames(true));
    await new Promise((r) => setTimeout(r, 20));

    expect(result.current('custom-ai-3f2a')).toBe('custom-ai-3f2a');
  });

  it('ignores a payload that is not a status', async () => {
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, ['not', 'a', 'status'])));

    const { result } = renderHook(() => useProviderNames(true));
    await new Promise((r) => setTimeout(r, 20));

    expect(result.current('ollama')).toBe('ollama');
  });
});
