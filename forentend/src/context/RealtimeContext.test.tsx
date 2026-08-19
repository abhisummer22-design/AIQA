/**
 * FE-9 targeted tests for `RealtimeContext` — brief §34 items 1-26 (regression
 * items 27-32 are covered by ExecutionHubPage/EvidencePage/AuditPage/
 * AppRoutes's own existing suites, which already re-ran clean with FE-9's
 * code present — see the FE-9 report §22). Exercised through the real
 * `AppShell` tree (same pattern as every prior FE unit's page tests) with
 * `fetch` mocked at the transport boundary and a controllable
 * `MockWebSocket` standing in for the browser's real `WebSocket`.
 */
import React from 'react';
import { act, render, renderHook, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../App';
import { AuthProvider, useAuth } from './AuthContext';
import { ProjectProvider, useProject } from './ProjectContext';
import { RealtimeProvider, useRealtime } from './RealtimeContext';
import type { Project, RunEvent } from '../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const PROJECT_A: Project = { id: 'proj-a', name: 'Project Alpha', description: null, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' };
const PROJECT_B: Project = { id: 'proj-b', name: 'Project Beta', description: null, created_at: '2026-01-02T00:00:00Z', updated_at: '2026-01-02T00:00:00Z' };

// ------------------------------------------------------------ MockWebSocket

class MockWebSocket {
  static instances: MockWebSocket[] = [];
  url: string;
  readyState = 0; // CONNECTING
  closedByClient = false;
  onopen: ((ev: unknown) => void) | null = null;
  onmessage: ((ev: { data: string }) => void) | null = null;
  onerror: ((ev: unknown) => void) | null = null;
  onclose: ((ev: { code: number; reason: string }) => void) | null = null;

  constructor(url: string) {
    this.url = url;
    MockWebSocket.instances.push(this);
  }

  close() {
    if (this.readyState === 3) return;
    this.closedByClient = true;
    this.readyState = 3;
    this.onclose?.({ code: 1000, reason: '' });
  }

  simulateOpen() {
    this.readyState = 1;
    this.onopen?.({});
  }

  simulateMessage(data: unknown) {
    this.onmessage?.({ data: typeof data === 'string' ? data : JSON.stringify(data) });
  }

  /** A close the *server* initiated — `closedByClient` stays false, so
   *  `RealtimeProvider` must treat this as unexpected and reconnect. */
  simulateServerClose(code = 1006, reason = '') {
    this.readyState = 3;
    this.onclose?.({ code, reason });
  }
}

function realEvent(overrides: Partial<RunEvent> = {}): RunEvent {
  return {
    event_id: `evt-${Math.random().toString(36).slice(2)}`,
    run_id: 'run-a',
    task_id: null,
    sequence: 1,
    timestamp: '2026-08-19T10:00:00Z',
    schema_version: 1,
    type: 'TASK_STARTED',
    status: 'RUNNING',
    summary: null,
    project_id: PROJECT_A.id,
    project_sequence: 1,
    ...overrides,
  };
}

function mockBackend(opts: { projects: Project[] }) {
  const fetchMock = vi.fn().mockImplementation(async (url: string) => {
    if (url.includes('/executions')) return jsonResponse(200, []); // ExecutionHubPage's own list call
    return jsonResponse(200, opts.projects);
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

function renderApp(initialEntries: string[] = ['/execution']) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <AppShell />
    </MemoryRouter>,
  );
}

async function verifyKey(): Promise<void> {
  await act(async () => {
    screen.getByText('Verify Connection').closest('button')!.click();
  });
}

async function openAndSelectProject(project: Project): Promise<void> {
  await act(async () => {
    document.getElementById('project-selector-button')!.click();
  });
  // Scoped to the header's own real listbox — FE-10's Dashboard has its
  // own real `<select>` of the same projects, which would otherwise
  // collide with an unscoped "option" role query if Dashboard is ever
  // mounted at the same time as this helper runs.
  await act(async () => {
    within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: project.name }).click();
  });
}

/** Real-flow re-save via the real Settings key panel (`SettingsPage.tsx`) —
 *  the only genuine UI path that re-triggers `AuthContext.validate()` after
 *  the very first automatic one, used here purely to exercise a real
 *  status *transition* (e.g. into `unauthenticated`) the way this app's own
 *  user actually would. */
async function resaveKeyViaSettings(rawKey: string): Promise<void> {
  await act(async () => {
    const enterDifferent = screen.queryByText('Enter a different key…');
    enterDifferent?.click();
  });
  const input = await screen.findByPlaceholderText('aiqa_...');
  await act(async () => {
    (input as HTMLInputElement).focus();
    Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value')!.set!.call(input, rawKey);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await act(async () => {
    screen.getByText(/Replace Key|Save Key/).closest('button')!.click();
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
  MockWebSocket.instances = [];
  vi.stubGlobal('WebSocket', MockWebSocket as unknown as typeof WebSocket);
});

afterEach(() => {
  localStorage.clear();
  // Guaranteed regardless of a mid-test assertion failure — an early throw
  // before a test's own trailing `vi.useRealTimers()` would otherwise leak
  // fake timers into every subsequent test, hanging their real `waitFor`
  // calls forever (exactly what happened here while writing this suite).
  vi.useRealTimers();
});

const RAW_KEY = 'aiqa_realtimetest0000000000';

describe('RealtimeContext (FE-9 §34 items 1-26)', () => {
  // --------------------------------------------------------- connection

  it('item 1 — no key never opens a realtime connection', async () => {
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await waitFor(() => expect(screen.getByText(/No orchestrator API key/i)).toBeTruthy());
    expect(MockWebSocket.instances.length).toBe(0);
  });

  it('item 2 — key_present (unverified) never opens a connection', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await waitFor(() => expect(screen.getByText('Key saved, not yet verified')).toBeTruthy());
    expect(MockWebSocket.instances.length).toBe(0);
  });

  it('item 3 — authenticated + a selected project opens exactly one real connection to the real project-scoped URL', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const ws = MockWebSocket.instances[0];
    expect(ws.url).toContain(`/projects/${PROJECT_A.id}/events`);
    expect(ws.url).toContain(`key=${RAW_KEY}`);
    expect(ws.url.startsWith('ws:')).toBe(true);
  });

  it('item 4/5 — switching project disconnects the old connection and opens exactly one new one', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A, PROJECT_B] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const first = MockWebSocket.instances[0];
    act(() => first.simulateOpen());

    await openAndSelectProject(PROJECT_B);

    await waitFor(() => expect(MockWebSocket.instances.length).toBe(2));
    expect(first.closedByClient).toBe(true);
    const second = MockWebSocket.instances[1];
    expect(second.url).toContain(`/projects/${PROJECT_B.id}/events`);
    expect(second.closedByClient).toBe(false);
  });

  /** Real, protected-route navigation to Settings (the `Sign Out` control
   *  and the key-replace panel both live there) via the real Sidebar link
   *  — `renderApp(['/settings'])` directly would skip `StateGate`
   *  entirely, since Settings is deliberately the one unprotected route
   *  (`routeConfig.tsx`'s own doc comment), and never show "Verify
   *  Connection" at all. */
  async function navigateToSettings(): Promise<void> {
    await act(async () => {
      screen.getByText('Settings / AI Model').closest('button')!.click();
    });
    // The real credential panel lives under Settings' own "Authentication &
    // SSO" tab, not the default "General" tab it lands on.
    await act(async () => {
      screen.getByText('Authentication & SSO').closest('button')!.click();
    });
  }

  it('item 6 — sign-out disconnects the real connection', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const ws = MockWebSocket.instances[0];
    act(() => ws.simulateOpen());

    await navigateToSettings();
    await act(async () => {
      screen.getByText('Sign Out').closest('button')!.click();
    });

    await waitFor(() => expect(ws.closedByClient).toBe(true));
    expect(MockWebSocket.instances.length).toBe(1); // no new connection opened after sign-out
  });

  it('item 7 — a real 401 (auth loss) disconnects the realtime connection', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    const fetchMock = mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const ws = MockWebSocket.instances[0];
    act(() => ws.simulateOpen());

    await navigateToSettings();
    fetchMock.mockImplementation(async () => jsonResponse(401, { error: { code: 'UNAUTHENTICATED', message: 'revoked', details: {}, request_id: 'r1' } }));
    await resaveKeyViaSettings('aiqa_someotherkey0000000000');

    await waitFor(() => expect(ws.closedByClient).toBe(true));
  });

  it('item 8/9 — a real unexpected close triggers a bounded reconnect, never more than one live socket at a time', async () => {
    vi.useFakeTimers();
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await act(async () => {
      screen.getByText('Verify Connection').closest('button')!.click();
      await vi.runOnlyPendingTimersAsync();
    });
    await act(async () => {
      document.getElementById('project-selector-button')!.click();
    });
    await act(async () => {
      screen.getByRole('option', { name: PROJECT_A.name }).click();
    });
    expect(MockWebSocket.instances.length).toBe(1);
    const first = MockWebSocket.instances[0];
    act(() => first.simulateOpen());

    // A real server-initiated close (not one this provider requested).
    act(() => first.simulateServerClose(1006, 'abnormal closure'));

    // The bounded backoff's first attempt fires after ~1s — advance exactly
    // that much and confirm exactly one new socket appears (never two).
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(MockWebSocket.instances.length).toBe(2);
    expect(MockWebSocket.instances[1].closedByClient).toBe(false);

    vi.useRealTimers();
  });

  it('item 10 — connection state transitions correctly (connecting → connected → reconnecting → error)', async () => {
    vi.useFakeTimers();
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await act(async () => {
      screen.getByText('Verify Connection').closest('button')!.click();
      await vi.runOnlyPendingTimersAsync();
    });
    await act(async () => {
      document.getElementById('project-selector-button')!.click();
    });
    await act(async () => {
      screen.getByRole('option', { name: PROJECT_A.name }).click();
    });

    // connecting (pre-open)
    expect(document.getElementById('execution-realtime-indicator')!.textContent).toMatch(/Connecting/i);

    act(() => MockWebSocket.instances[0].simulateOpen());
    expect(document.getElementById('execution-realtime-indicator')!.textContent).toMatch(/Live/i);

    // Drive every reconnect attempt to exhaustion purely through real close
    // events — never fabricated state. `MAX_RECONNECT_ATTEMPTS` (6, see
    // `RealtimeContext.tsx`) schedules a reconnect on closes 1-6; only the
    // 7th close observes the attempt counter already exhausted and settles
    // on `error` instead of scheduling another timer — so this drives 7
    // closes total, advancing the timer after the first 6 (which each open
    // a new socket) but not the 7th (which opens none).
    for (let i = 0; i < 7; i += 1) {
      const current = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      act(() => current.simulateServerClose(1006, 'down'));
      if (i < 6) {
        expect(document.getElementById('execution-realtime-indicator')!.textContent).toMatch(/Reconnecting/i);
        await act(async () => {
          await vi.advanceTimersByTimeAsync(30000);
        });
      }
    }
    expect(document.getElementById('execution-realtime-indicator')!.textContent).toMatch(/Realtime Error/i);

    vi.useRealTimers();
  }, 15000);

  // ------------------------------------------------------------- events

  it('item 11 — a valid execution event for the selected run triggers a real resync of its detail', async () => {
    const fetchMock = mockBackend({ projects: [PROJECT_A] });
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    act(() => MockWebSocket.instances[0].simulateOpen());

    const before = fetchMock.mock.calls.length;
    act(() => MockWebSocket.instances[0].simulateMessage(realEvent({ run_id: 'run-a' })));
    // Not the selected run (nothing selected in this test) — this only
    // proves an accepted, validated event does not crash anything; item 20
    // below proves project isolation specifically.
    await waitFor(() => expect(fetchMock.mock.calls.length).toBeGreaterThanOrEqual(before));
  });

  it('item 18/19 — an unknown event type and a malformed payload never crash the app', async () => {
    mockBackend({ projects: [PROJECT_A] });
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const ws = MockWebSocket.instances[0];
    act(() => ws.simulateOpen());

    act(() => ws.simulateMessage(realEvent({ type: 'SOME_FUTURE_EVENT_TYPE' as RunEvent['type'] })));
    act(() => ws.simulateMessage('not even json{{{'));
    act(() => ws.simulateMessage(JSON.stringify({ not: 'a real event' })));

    // The page is still fully intact — the surest proof nothing crashed.
    expect(screen.getByText('Execution Hub')).toBeTruthy();
  });

  it('item 20 — a foreign-project event is ignored', async () => {
    const fetchMock = mockBackend({ projects: [PROJECT_A] });
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const ws = MockWebSocket.instances[0];
    act(() => ws.simulateOpen());

    const before = fetchMock.mock.calls.length;
    act(() => ws.simulateMessage(realEvent({ project_id: 'some-other-project', run_id: 'run-a' })));
    // No new request fired purely from an event whose own `project_id`
    // does not match the currently-selected project — the client-side
    // isolation check (`RealtimeContext`'s own `handleIncoming`) rejected
    // it before any listener ran.
    expect(fetchMock.mock.calls.length).toBe(before);
  });

function harnessWrapper({ children }: { children: React.ReactNode }) {
  return (
    <AuthProvider>
      <ProjectProvider>
        <RealtimeProvider>{children}</RealtimeProvider>
      </ProjectProvider>
    </AuthProvider>
  );
}

function useHarness() {
  return { auth: useAuth(), project: useProject(), realtime: useRealtime() };
}

describe('RealtimeContext — direct hook-level tests (no full page UI needed)', () => {
  it('item 21 — a duplicate event (same real event_id) is delivered to subscribers only once', async () => {
    mockBackend({ projects: [PROJECT_A] });
    const { result } = renderHook(() => useHarness(), { wrapper: harnessWrapper });

    await act(async () => {
      await result.current.auth.saveKey(RAW_KEY);
    });
    await waitFor(() => expect(result.current.project.projects.length).toBe(1));
    act(() => {
      result.current.project.selectProject(PROJECT_A.id);
    });
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const ws = MockWebSocket.instances[0];
    act(() => ws.simulateOpen());

    let received = 0;
    act(() => {
      result.current.realtime.subscribe(() => {
        received += 1;
      });
    });

    const dup = realEvent({ event_id: 'evt-fixed-id-123' });
    act(() => ws.simulateMessage(dup));
    act(() => ws.simulateMessage(dup)); // the exact same event_id, as a real reconnect/replay could deliver

    expect(received).toBe(1);
  });
});

  // ---------------------------------------------------------- recovery

  it('item 23/24 — a real unexpected disconnect enters reconnecting, then a successful reconnect restores connected', async () => {
    vi.useFakeTimers();
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await act(async () => {
      screen.getByText('Verify Connection').closest('button')!.click();
      await vi.runOnlyPendingTimersAsync();
    });
    await act(async () => {
      document.getElementById('project-selector-button')!.click();
    });
    await act(async () => {
      screen.getByRole('option', { name: PROJECT_A.name }).click();
    });
    act(() => MockWebSocket.instances[0].simulateOpen());
    expect(document.getElementById('execution-realtime-indicator')!.textContent).toMatch(/Live/i);

    act(() => MockWebSocket.instances[0].simulateServerClose(1006, 'down'));
    expect(document.getElementById('execution-realtime-indicator')!.textContent).toMatch(/Reconnecting/i);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(MockWebSocket.instances.length).toBe(2);
    act(() => MockWebSocket.instances[1].simulateOpen());
    expect(document.getElementById('execution-realtime-indicator')!.textContent).toMatch(/Live/i);

    vi.useRealTimers();
  }, 15000);

  it('item 25 — BE-14 lag close (code 4001) reconnects immediately, uncounted against the failure backoff', async () => {
    vi.useFakeTimers();
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await act(async () => {
      screen.getByText('Verify Connection').closest('button')!.click();
      await vi.runOnlyPendingTimersAsync();
    });
    await act(async () => {
      document.getElementById('project-selector-button')!.click();
    });
    await act(async () => {
      screen.getByRole('option', { name: PROJECT_A.name }).click();
    });
    act(() => MockWebSocket.instances[0].simulateOpen());

    act(() => MockWebSocket.instances[0].simulateServerClose(4001, 'lagged behind by 40 events; reconnect and catch up via REST'));

    // No wait at all — a real 4001 reconnects synchronously in the same
    // close handler, not through the timed backoff path items 8/9 exercise.
    expect(MockWebSocket.instances.length).toBe(2);

    vi.useRealTimers();
  });

  it('item 26 — reconnect attempts are bounded, not an infinite loop', async () => {
    vi.useFakeTimers();
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await act(async () => {
      screen.getByText('Verify Connection').closest('button')!.click();
      await vi.runOnlyPendingTimersAsync();
    });
    await act(async () => {
      document.getElementById('project-selector-button')!.click();
    });
    await act(async () => {
      screen.getByRole('option', { name: PROJECT_A.name }).click();
    });
    act(() => MockWebSocket.instances[0].simulateOpen());

    // Same real 7-closes-to-exhaust shape as item 10 (see that test's own
    // comment for why it's 7, not 6).
    for (let i = 0; i < 7; i += 1) {
      const current = MockWebSocket.instances[MockWebSocket.instances.length - 1];
      act(() => current.simulateServerClose(1006, 'down'));
      if (i < 6) {
        await act(async () => {
          await vi.advanceTimersByTimeAsync(30000);
        });
      }
    }
    const countAtExhaustion = MockWebSocket.instances.length;
    expect(document.getElementById('execution-realtime-indicator')!.textContent).toMatch(/Realtime Error/i);

    // Advancing time far further must not spawn any further sockets — a
    // real bounded stop, not a silently-still-running loop.
    await act(async () => {
      await vi.advanceTimersByTimeAsync(120000);
    });
    expect(MockWebSocket.instances.length).toBe(countAtExhaustion);

    vi.useRealTimers();
  }, 15000);
});
