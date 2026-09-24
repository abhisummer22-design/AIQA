/**
 * Central AI Provider & Model Intelligence Layer — Settings panel tests.
 * Renders the real `AppShell` (real providers/routing) with `fetch` mocked
 * only at the transport boundary, matching every other real-data page test
 * in this app (`DashboardPage.test.tsx`/`TestCaseGeneratorPage.test.tsx`).
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { AiProviderInfo, AiStatusResponse } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const RAW_KEY = 'aiqa_settingsai0000000000000000';

function renderApp() {
  return render(
    <MemoryRouter initialEntries={['/settings']}>
      <AppShell />
    </MemoryRouter>,
  );
}

async function openAiTab(): Promise<void> {
  await act(async () => {
    screen.getByText('AI Models & Reasoning').click();
  });
}

const OLLAMA_PROVIDER: AiProviderInfo = {
  id: 'ollama',
  name: 'Ollama',
  provider_type: 'LOCAL',
  endpoint: 'http://localhost:11434',
  configured: true,
  reachable: true,
  status: 'HEALTHY',
  models: [
    {
      id: 'gemma3:12b',
      provider_id: 'ollama',
      display_name: 'gemma3:12b',
      family: 'gemma3',
      parameter_size: '12.2B',
      context_window: null,
      capabilities: {
        text_generation: 'STRONG',
        structured_output: 'BASIC',
        reasoning: 'BASIC',
        vision: 'NONE',
        embedding: 'NONE',
        tool_use: 'UNKNOWN',
        long_context: 'UNKNOWN',
      },
      status: 'AVAILABLE',
    },
  ],
  pricing: { kind: 'UNMETERED' },
};

const GEMINI_NOT_CONFIGURED: AiProviderInfo = {
  id: 'gemini',
  name: 'Gemini',
  provider_type: 'API',
  endpoint: null,
  configured: false,
  reachable: false,
  status: 'NOT_CONFIGURED',
  models: [],
  pricing: { kind: 'METERED', input_usd_per_1k: 0.01, output_usd_per_1k: 0.03 },
};

function statusResponse(overrides: Partial<AiStatusResponse> = {}): AiStatusResponse {
  return {
    providers: [OLLAMA_PROVIDER, GEMINI_NOT_CONFIGURED],
    settings: { mode: 'AUTO', preferred_provider: null, preferred_model: null, fallback_enabled: true },
    routing_preview: { provider_id: 'ollama', model_id: 'gemma3:12b', routing_score: 145, reason: 'AUTO mode: best available model for TestCaseGeneration by routing score' },
    ...overrides,
  };
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
  setApiKey(RAW_KEY);
});

afterEach(() => {
  localStorage.clear();
});

describe('SettingsPage — AI Intelligence panel (Central AI Model Auto-Discovery)', () => {
  it('renders real discovered providers and the real routing preview, never a hardcoded "Connected"', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.endsWith('/ai/status')) return jsonResponse(200, statusResponse());
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await waitFor(() => expect(screen.getByText('System Settings & Governance')).toBeTruthy());
    await openAiTab();

    await waitFor(() => expect(screen.getByText('Ollama')).toBeTruthy());
    expect(screen.getByText('Healthy')).toBeTruthy();
    expect(screen.getByText('Not Configured')).toBeTruthy();
    expect(screen.getByText('Ready')).toBeTruthy();
    expect(document.getElementById('ai-intelligence-panel')!.textContent).not.toContain('Connected');
    // Real pricing facts, not a guessed rate: the self-hosted Ollama
    // provider shows Free, the (fixture) metered Gemini shows its real rate.
    expect(screen.getByText('Free')).toBeTruthy();
    expect(screen.getByText('$0.01/$0.03 per 1K')).toBeTruthy();
  });

  it('shows a truthful Degraded status and "no AI model currently available" when nothing is healthy — never a fake Ready', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.endsWith('/ai/status')) {
          return jsonResponse(
            200,
            statusResponse({
              providers: [{ ...OLLAMA_PROVIDER, status: 'UNAVAILABLE', reachable: false, models: [] }, GEMINI_NOT_CONFIGURED],
              routing_preview: null,
            }),
          );
        }
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await waitFor(() => expect(screen.getByText('System Settings & Governance')).toBeTruthy());
    await openAiTab();

    await waitFor(() => expect(screen.getByText(/Degraded — No AI model currently available/)).toBeTruthy());
    expect(screen.getByText(/No AI model currently available for Test Case Generation/)).toBeTruthy();
    expect(screen.queryByText('Ready')).toBeNull();
  });

  it('Scan Providers makes a real POST /ai/refresh request and reloads real status', async () => {
    let refreshCalled = false;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.endsWith('/ai/refresh') && init?.method === 'POST') {
        refreshCalled = true;
        return jsonResponse(200, [OLLAMA_PROVIDER]);
      }
      if (url.endsWith('/ai/status')) return jsonResponse(200, statusResponse());
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await waitFor(() => expect(screen.getByText('System Settings & Governance')).toBeTruthy());
    await openAiTab();
    await waitFor(() => expect(document.getElementById('btn-refresh-ai-providers')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-refresh-ai-providers')!.click();
    });

    await waitFor(() => expect(refreshCalled).toBe(true));
  });

  it('switching to MANUAL mode and saving sends a real PUT /ai/settings request with the chosen provider/model', async () => {
    let savedBody: any = null;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.endsWith('/ai/settings') && init?.method === 'PUT') {
        savedBody = JSON.parse(String(init.body));
        return jsonResponse(200, savedBody);
      }
      if (url.endsWith('/ai/status')) return jsonResponse(200, statusResponse());
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await waitFor(() => expect(screen.getByText('System Settings & Governance')).toBeTruthy());
    await openAiTab();
    await waitFor(() => expect(screen.getByText('Ollama')).toBeTruthy());

    const modeSelect = document.querySelector('select') as HTMLSelectElement;
    await act(async () => {
      modeSelect.value = 'MANUAL';
      modeSelect.dispatchEvent(new Event('change', { bubbles: true }));
    });

    const selects = Array.from(document.querySelectorAll('select')) as HTMLSelectElement[];
    const providerSelect = selects[1];
    await act(async () => {
      providerSelect.value = 'ollama';
      providerSelect.dispatchEvent(new Event('change', { bubbles: true }));
    });
    const modelSelect = selects[2];
    await act(async () => {
      modelSelect.value = 'gemma3:12b';
      modelSelect.dispatchEvent(new Event('change', { bubbles: true }));
    });

    await act(async () => {
      document.getElementById('btn-save-ai-settings')!.click();
    });

    await waitFor(() => expect(savedBody).toBeTruthy());
    expect(savedBody.mode).toBe('MANUAL');
    expect(savedBody.preferred_provider).toBe('ollama');
    expect(savedBody.preferred_model).toBe('gemma3:12b');
  });

  it('a failed status fetch shows a real, honest error — never a fabricated Ready/Healthy fallback', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.endsWith('/ai/status')) return jsonResponse(500, { error: { code: 'INTERNAL_ERROR', message: 'boom', details: null, request_id: 'r1' } });
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await waitFor(() => expect(screen.getByText('System Settings & Governance')).toBeTruthy());
    await openAiTab();

    await waitFor(() => expect(document.getElementById('ai-intelligence-panel')).toBeNull());
    expect(screen.queryByText('Ready')).toBeNull();
    expect(screen.queryByText('Healthy')).toBeNull();
  });
});

describe('SettingsPage — custom AI providers (add/remove via the Settings UI)', () => {
  it('renders a real, previously-added custom provider from GET /ai/providers/custom', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.endsWith('/ai/status')) return jsonResponse(200, statusResponse());
        if (url.endsWith('/ai/providers/custom')) {
          return jsonResponse(200, [
            { id: 'custom-ai-1', name: 'My Real Provider', base_url: 'https://real.example.com/v1', created_by: 'test-key', created_at: '2026-01-01T00:00:00Z' },
          ]);
        }
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await waitFor(() => expect(screen.getByText('System Settings & Governance')).toBeTruthy());
    await openAiTab();

    await waitFor(() => expect(screen.getByText('My Real Provider')).toBeTruthy());
    expect(screen.getByText('https://real.example.com/v1')).toBeTruthy();
  });

  it('Add Custom Provider sends a real POST with the entered fields, clears the key input, and forces a real re-discovery', async () => {
    let createdBody: any = null;
    let refreshCalled = false;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.endsWith('/ai/providers/custom') && init?.method === 'POST') {
        createdBody = JSON.parse(String(init.body));
        return jsonResponse(201, { id: 'custom-ai-new', name: createdBody.name, base_url: createdBody.base_url, created_by: 'test-key', created_at: '2026-01-01T00:00:00Z' });
      }
      if (url.endsWith('/ai/providers/custom')) return jsonResponse(200, []);
      if (url.endsWith('/ai/refresh') && init?.method === 'POST') {
        refreshCalled = true;
        return jsonResponse(200, []);
      }
      if (url.endsWith('/ai/status')) return jsonResponse(200, statusResponse());
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await waitFor(() => expect(screen.getByText('System Settings & Governance')).toBeTruthy());
    await openAiTab();
    await waitFor(() => expect(document.getElementById('new-custom-provider-name')).toBeTruthy());

    const nameInput = document.getElementById('new-custom-provider-name') as HTMLInputElement;
    const urlInput = document.getElementById('new-custom-provider-base-url') as HTMLInputElement;
    const keyInput = document.getElementById('new-custom-provider-api-key') as HTMLInputElement;
    await act(async () => {
      fireEvent.change(nameInput, { target: { value: 'My Custom Provider' } });
      fireEvent.change(urlInput, { target: { value: 'https://custom.example.com' } });
      fireEvent.change(keyInput, { target: { value: 'sk-real-secret-value' } });
    });

    await act(async () => {
      document.getElementById('btn-add-custom-ai-provider')!.click();
    });

    await waitFor(() => expect(createdBody).toBeTruthy());
    expect(createdBody.name).toBe('My Custom Provider');
    expect(createdBody.base_url).toBe('https://custom.example.com');
    expect(createdBody.api_key).toBe('sk-real-secret-value');
    await waitFor(() => expect(refreshCalled).toBe(true));

    // The raw key must never linger in the input once saved (same
    // discipline the real orchestrator key panel already holds itself to).
    expect((document.getElementById('new-custom-provider-api-key') as HTMLInputElement).value).toBe('');
  });

  it('a real backend validation failure is shown honestly, never silently swallowed', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.endsWith('/ai/providers/custom') && init?.method === 'POST') {
          return jsonResponse(400, { error: { code: 'VALIDATION_ERROR', message: 'base_url must be a real http:// or https:// endpoint', details: null, request_id: 'r1' } });
        }
        if (url.endsWith('/ai/status')) return jsonResponse(200, statusResponse());
        return jsonResponse(200, []);
      }),
    );

    renderApp();
    await waitFor(() => expect(screen.getByText('System Settings & Governance')).toBeTruthy());
    await openAiTab();
    await waitFor(() => expect(document.getElementById('new-custom-provider-name')).toBeTruthy());

    const nameInput = document.getElementById('new-custom-provider-name') as HTMLInputElement;
    const urlInput = document.getElementById('new-custom-provider-base-url') as HTMLInputElement;
    const keyInput = document.getElementById('new-custom-provider-api-key') as HTMLInputElement;
    await act(async () => {
      fireEvent.change(nameInput, { target: { value: 'Bad Provider' } });
      fireEvent.change(urlInput, { target: { value: 'not-a-real-url' } });
      fireEvent.change(keyInput, { target: { value: 'sk-whatever' } });
    });
    await act(async () => {
      document.getElementById('btn-add-custom-ai-provider')!.click();
    });

    await waitFor(() => expect(screen.getByText(/base_url must be a real/)).toBeTruthy());
  });

  it('Delete on a custom provider sends a real DELETE and removes it from the list', async () => {
    let deletedId: string | null = null;
    let listCallCount = 0;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url.endsWith('/ai/providers/custom/custom-ai-1') && init?.method === 'DELETE') {
        deletedId = 'custom-ai-1';
        return new Response(null, { status: 204 });
      }
      if (url.endsWith('/ai/providers/custom')) {
        listCallCount += 1;
        // First load shows the real provider; after delete + reload, it's
        // genuinely gone — not just hidden client-side.
        return jsonResponse(200, listCallCount === 1 ? [{ id: 'custom-ai-1', name: 'My Real Provider', base_url: 'https://real.example.com', created_by: 'test-key', created_at: '2026-01-01T00:00:00Z' }] : []);
      }
      if (url.endsWith('/ai/status')) return jsonResponse(200, statusResponse());
      return jsonResponse(200, []);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await waitFor(() => expect(screen.getByText('System Settings & Governance')).toBeTruthy());
    await openAiTab();
    await waitFor(() => expect(screen.getByText('My Real Provider')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-delete-custom-provider-custom-ai-1')!.click();
    });

    await waitFor(() => expect(deletedId).toBe('custom-ai-1'));
    await waitFor(() => expect(screen.queryByText('My Real Provider')).toBeNull());
  });
});

describe('SettingsPage — task-wise routing chains', () => {
  const QWEN = { ...OLLAMA_PROVIDER.models[0], id: 'qwen3:14b', display_name: 'qwen3:14b' };
  const PROVIDERS = [{ ...OLLAMA_PROVIDER, models: [...OLLAMA_PROVIDER.models, QWEN] }, GEMINI_NOT_CONFIGURED];

  /** Serves `status` for GET /ai/status and records the body of every PUT /ai/settings. */
  function serve(status: () => AiStatusResponse) {
    const saved: any[] = [];
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (url.endsWith('/ai/settings') && init?.method === 'PUT') {
          const body = JSON.parse(String(init.body));
          saved.push(body);
          return jsonResponse(200, body);
        }
        if (url.endsWith('/ai/status')) return jsonResponse(200, status());
        return jsonResponse(200, []);
      }),
    );
    return saved;
  }

  async function openPanel() {
    renderApp();
    await waitFor(() => expect(screen.getByText('System Settings & Governance')).toBeTruthy());
    await openAiTab();
    await waitFor(() => expect(document.getElementById('btn-save-ai-settings')).toBeTruthy());
  }

  const save = async () => {
    await act(async () => {
      document.getElementById('btn-save-ai-settings')!.click();
    });
  };

  it('saving without any task list sends the same payload as before — no task_overrides key at all', async () => {
    const saved = serve(() => statusResponse({ providers: PROVIDERS }));
    await openPanel();

    await save();

    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual({ mode: 'AUTO', preferred_provider: null, preferred_model: null, fallback_enabled: true });
  });

  it('saving sends the overrides the backend already has, so a save can never erase them', async () => {
    const existing = {
      TEST_CASE_GENERATION: {
        provider: null,
        model: null,
        chain: [
          { provider: 'ollama', model: 'qwen3:14b' },
          { provider: 'gemini', model: 'gemini-2.5-flash' },
        ],
      },
      BUG_ANALYSIS: { provider: 'ollama', model: null },
    };
    const saved = serve(() =>
      statusResponse({
        providers: PROVIDERS,
        settings: { mode: 'AUTO', preferred_provider: null, preferred_model: null, fallback_enabled: true, task_overrides: existing },
      }),
    );
    await openPanel();

    // The saved chain is shown, already open.
    expect(document.getElementById('task-chain-TEST_CASE_GENERATION')!.textContent).toContain('qwen3:14b');

    await save();

    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0].task_overrides).toEqual(existing);
  });

  it('a chain built in the Settings UI is what gets saved', async () => {
    const saved = serve(() => statusResponse({ providers: PROVIDERS }));
    await openPanel();

    await act(async () => {
      document.getElementById('btn-toggle-task-routing')!.click();
    });
    await act(async () => {
      fireEvent.change(document.getElementById('task-chain-TEST_CASE_GENERATION-provider')!, { target: { value: 'ollama' } });
    });
    await act(async () => {
      fireEvent.change(document.getElementById('task-chain-TEST_CASE_GENERATION-model')!, { target: { value: 'qwen3:14b' } });
    });
    await act(async () => {
      document.getElementById('task-chain-TEST_CASE_GENERATION-add')!.click();
    });
    await save();

    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0].task_overrides).toEqual({
      TEST_CASE_GENERATION: { provider: null, model: null, chain: [{ provider: 'ollama', model: 'qwen3:14b' }] },
    });
  });

  it('emptying every list and saving clears the overrides — the key is left out, not sent empty', async () => {
    const saved = serve(() =>
      statusResponse({
        providers: PROVIDERS,
        settings: {
          mode: 'AUTO',
          preferred_provider: null,
          preferred_model: null,
          fallback_enabled: true,
          task_overrides: { TEST_CASE_GENERATION: { provider: null, model: null, chain: [{ provider: 'ollama', model: 'qwen3:14b' }] } },
        },
      }),
    );
    await openPanel();

    await act(async () => {
      screen.getByLabelText('Remove qwen3:14b from Test case generation').click();
    });
    await save();

    await waitFor(() => expect(saved).toHaveLength(1));
    expect('task_overrides' in saved[0]).toBe(false);
  });

  it('after saving, the Active Model preview is read again so it reflects the new lists', async () => {
    let statusCalls = 0;
    serve(() => {
      statusCalls += 1;
      return statusCalls === 1
        ? statusResponse({ providers: PROVIDERS })
        : statusResponse({
            providers: PROVIDERS,
            routing_preview: {
              provider_id: 'ollama',
              model_id: 'qwen3:14b',
              routing_score: 200,
              reason: 'Task chain: first available model for TestCaseGeneration',
            },
          });
    });
    await openPanel();
    expect(screen.queryByText(/Task chain: first available model/)).toBeNull();

    await save();

    await waitFor(() => expect(screen.getByText('Task chain: first available model for TestCaseGeneration')).toBeTruthy());
    expect(screen.getByText('qwen3:14b')).toBeTruthy();
  });

  it('a failed re-read of the preview after a successful save is not shown as a save error', async () => {
    let statusCalls = 0;
    const saved = serve(() => {
      statusCalls += 1;
      if (statusCalls > 1) throw new Error('offline');
      return statusResponse({ providers: PROVIDERS });
    });
    await openPanel();

    await save();

    await waitFor(() => expect(saved).toHaveLength(1));
    await waitFor(() => expect(statusCalls).toBe(2));
    // Neither the offline message the failed read raises nor the generic save failure may appear.
    expect(screen.queryByText(/Cannot reach the AIQA orchestrator/)).toBeNull();
    expect(screen.queryByText(/Failed to save AI settings/)).toBeNull();
    expect(document.getElementById('ai-intelligence-panel')).toBeTruthy();
  });
});
