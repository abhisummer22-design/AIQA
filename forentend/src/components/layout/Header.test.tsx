/**
 * FE-24 targeted tests for the real Header notification bell —
 * `RealtimeContext`-derived recent activity, replacing the previous
 * fabricated "Recent Quality Alerts" / "Live Stream" panel and the
 * hardcoded `notificationCount = 3`. Same real-`AppShell` /
 * `MockWebSocket`-for-`RealtimeContext` pattern `RealtimeContext.test.tsx`
 * already established — reused here, not reinvented, since Header is
 * simply another real subscriber of the same real, already project-
 * isolated, already-deduplicated event stream.
 */
import React from 'react';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import type { Project, RunEvent } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const PROJECT_A: Project = { id: 'proj-a', name: 'Project Alpha', description: null, created_at: '2026-01-01T00:00:00Z', updated_at: '2026-01-01T00:00:00Z' };
const PROJECT_B: Project = { id: 'proj-b', name: 'Project Beta', description: null, created_at: '2026-01-02T00:00:00Z', updated_at: '2026-01-02T00:00:00Z' };

// ------------------------------------------------------------ MockWebSocket
// Identical shape to `RealtimeContext.test.tsx`'s own — duplicated locally
// per this codebase's established per-test-file convention.
class MockWebSocket {
  static instances: MockWebSocket[] = [];
  url: string;
  readyState = 0;
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
    if (url.includes('/executions')) return jsonResponse(200, []);
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
  const btn = screen.queryByText('Verify Connection')?.closest('button');
  if (btn) {
    await act(async () => {
      btn.click();
    });
  }
}

async function openAndSelectProject(project: Project): Promise<void> {
  await waitFor(() => expect(document.getElementById('project-selector-button')).toBeTruthy());
  await act(async () => {
    document.getElementById('project-selector-button')!.click();
  });
  await act(async () => {
    within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: project.name }).click();
  });
}

async function openBell(): Promise<void> {
  await act(async () => {
    document.getElementById('notifications-button')!.click();
  });
}

const RAW_KEY = 'aiqa_headertest0000000000000';

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
  MockWebSocket.instances = [];
  vi.stubGlobal('WebSocket', MockWebSocket as unknown as typeof WebSocket);
});

afterEach(() => {
  localStorage.clear();
});

describe('Header — FE-24 real recent-activity notification bell', () => {
  it('item 1 — no fabricated notification content exists anywhere, before or after opening the bell', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await openBell();

    for (const fabricated of [
      'Infrastructure DNS alert on Redis',
      'test-104 failed due to DNS timeout',
      'Auth Token Revocation Assertion',
      'Expected HTTP 401, received HTTP 200 on revoked cookie',
      'Recent Quality Alerts',
      'Live Stream',
    ]) {
      expect(screen.queryByText(fabricated)).toBeNull();
    }
  });

  it('item 2 — a truthful empty state renders before any realtime event has been received', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await openBell();

    expect(screen.getByText('No recent activity')).toBeTruthy();
    expect(screen.getByText('Real execution activity will appear here when events are received.')).toBeTruthy();
    expect(document.getElementById('notification-badge-dot')).toBeNull();
  });

  it('item 3 — a real RunEvent renders its real type, summary, and timestamp, with no fabricated values', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const ws = MockWebSocket.instances[0];
    act(() => ws.simulateOpen());

    act(() => ws.simulateMessage(realEvent({ type: 'TASK_FAILED', summary: 'task-42 failed: assertion error', run_id: 'run-real', timestamp: '2026-08-20T09:15:00Z' })));

    await openBell();
    expect(screen.getByText('TASK FAILED')).toBeTruthy();
    expect(screen.getByText('task-42 failed: assertion error')).toBeTruthy();
    expect(screen.getByText(/Run run-real/)).toBeTruthy();
  });

  it('item 4 — multiple real events render newest-first, bounded, with a count matching retained events', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const ws = MockWebSocket.instances[0];
    act(() => ws.simulateOpen());

    act(() => ws.simulateMessage(realEvent({ run_id: 'run-1st', type: 'TASK_STARTED', timestamp: '2026-08-20T09:00:00Z' })));
    act(() => ws.simulateMessage(realEvent({ run_id: 'run-2nd', type: 'TASK_PASSED', timestamp: '2026-08-20T09:01:00Z' })));
    act(() => ws.simulateMessage(realEvent({ run_id: 'run-3rd', type: 'RUN_COMPLETED', timestamp: '2026-08-20T09:02:00Z' })));

    await openBell();
    const panel = document.getElementById('recent-activity-panel')!;
    const rows = within(panel).getAllByText(/^Run run-/);
    expect(rows.length).toBe(3);
    // Newest first: run-3rd's own row must precede run-1st's.
    const order = rows.map((r) => r.textContent);
    expect(order[0]).toContain('run-3rd');
    expect(order[2]).toContain('run-1st');
  });

  it('item 5 — switching project clears the previous project\'s activity and shows only the new project\'s real events', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A, PROJECT_B] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const wsA = MockWebSocket.instances[0];
    act(() => wsA.simulateOpen());
    act(() => wsA.simulateMessage(realEvent({ run_id: 'run-a1', project_id: PROJECT_A.id })));

    await openBell();
    expect(screen.getByText(/Run run-a1/)).toBeTruthy();

    // The bell panel stays open across the switch (no toggle click here) —
    // exactly what proves the reset is real: the same open panel now shows
    // the new project's truthful empty state instead of A's stale event.
    await openAndSelectProject(PROJECT_B);
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(2));
    await waitFor(() => expect(screen.getByText('No recent activity')).toBeTruthy());
    expect(screen.queryByText(/run-a1/)).toBeNull();

    const wsB = MockWebSocket.instances[1];
    act(() => wsB.simulateOpen());
    act(() => wsB.simulateMessage(realEvent({ run_id: 'run-b1', project_id: PROJECT_B.id })));
    await waitFor(() => expect(screen.getByText(/Run run-b1/)).toBeTruthy());
    expect(screen.queryByText(/run-a1/)).toBeNull();
  });

  it('item 6 — the badge is absent (never a hardcoded 3) until a real event is actually received', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);

    expect(document.getElementById('notification-badge-dot')).toBeNull();

    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const ws = MockWebSocket.instances[0];
    act(() => ws.simulateOpen());
    act(() => ws.simulateMessage(realEvent()));

    await waitFor(() => expect(document.getElementById('notification-badge-dot')).toBeTruthy());
  });

  it('item 7 — the notification UI updates live from a real event with no reload/polling/manual refresh', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await openBell();
    expect(screen.getByText('No recent activity')).toBeTruthy();

    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const ws = MockWebSocket.instances[0];
    act(() => ws.simulateOpen());
    // No reload, no Refresh click, no timer — only the WS message itself
    // drives this update, exactly like every other real page's own
    // realtime-resync behavior already does.
    act(() => ws.simulateMessage(realEvent({ run_id: 'run-live', summary: 'live update test' })));

    await waitFor(() => expect(screen.getByText('live update test')).toBeTruthy());
  });

  it('item 8 — a duplicate event (same real event_id) never appears twice, matching RealtimeContext\'s own dedup guarantee', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();
    await openAndSelectProject(PROJECT_A);
    await waitFor(() => expect(MockWebSocket.instances.length).toBe(1));
    const ws = MockWebSocket.instances[0];
    act(() => ws.simulateOpen());

    const dup = realEvent({ run_id: 'run-dup', summary: 'duplicate-guarded event' });
    act(() => ws.simulateMessage(dup));
    act(() => ws.simulateMessage(dup)); // same event_id, sent twice

    await openBell();
    const panel = document.getElementById('recent-activity-panel')!;
    expect(within(panel).getAllByText('duplicate-guarded event').length).toBe(1);
  });
});

describe('Header — Target Environments selector (DEMO, UAT, LIVE only)', () => {
  it('renders DEMO, UAT, LIVE options and excludes STAGING and EPHEMERAL', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();

    const envButton = document.getElementById('env-selector-button')!;
    expect(envButton).toBeTruthy();
    // Default selected environment is DEMO
    expect(envButton.textContent).toContain('DEMO');

    // Open dropdown
    await act(async () => {
      envButton.click();
    });

    const menu = envButton.parentElement!;
    // Exactly DEMO, UAT, LIVE are present
    expect(within(menu).getByText('Target Environments')).toBeTruthy();
    expect(within(menu).getAllByText('DEMO').length).toBeGreaterThan(0);
    expect(within(menu).getByText('UAT')).toBeTruthy();
    expect(within(menu).getByText('LIVE')).toBeTruthy();

    // STAGING and EPHEMERAL are not present
    expect(within(menu).queryByText('Staging Cluster')).toBeNull();
    expect(within(menu).queryByText('Production Tier')).toBeNull();
    expect(within(menu).queryByText('UAT Acceptance')).toBeNull();
    expect(within(menu).queryByText('Ephemeral PR-1482')).toBeNull();
    expect(within(menu).queryByText('STAGING')).toBeNull();
    expect(within(menu).queryByText('EPHEMERAL')).toBeNull();
  });

  it('switching environment updates the selector label and badge', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();

    const envButton = document.getElementById('env-selector-button')!;
    expect(envButton.textContent).toContain('DEMO');

    // Open dropdown and select UAT
    await act(async () => {
      envButton.click();
    });
    const menu = envButton.parentElement!;
    await act(async () => {
      within(menu).getByText('UAT').click();
    });

    expect(envButton.textContent).toContain('UAT');
    expect(envButton.textContent).toContain('v4.18');

    // Open dropdown and select LIVE
    await act(async () => {
      envButton.click();
    });
    await act(async () => {
      within(menu).getByText('LIVE').click();
    });

    expect(envButton.textContent).toContain('LIVE');
    expect(envButton.textContent).toContain('US-East');
  });
});

describe('Header — Phase 6 gap-closing real theme toggle', () => {
  beforeEach(() => {
    document.documentElement.removeAttribute('data-theme');
  });

  it('renders dark by default and switches the real data-theme attribute on click, with no reload', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();

    const toggle = document.getElementById('theme-toggle-button')!;
    await waitFor(() => expect(toggle.getAttribute('title')).toBe('Switch to light mode'));
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');

    await act(async () => {
      toggle.click();
    });

    expect(document.documentElement.getAttribute('data-theme')).toBe('light');
    expect(toggle.getAttribute('title')).toBe('Switch to dark mode');
    expect(localStorage.getItem('aiqa.theme')).toBe('light');
  });

  it('a stored light preference is honored on load, before any click', async () => {
    localStorage.setItem('aiqa.orchestrator.apiKey', RAW_KEY);
    localStorage.setItem('aiqa.theme', 'light');
    mockBackend({ projects: [PROJECT_A] });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(document.documentElement.getAttribute('data-theme')).toBe('light'));
    expect(document.getElementById('theme-toggle-button')!.getAttribute('title')).toBe('Switch to dark mode');
  });
});

  describe("Header - exactly one project selector per breakpoint", () => {
    // The regression this guards: `ProjectSelector` was rendered twice, with
    // only the second copy wrapped in `hidden md:block`. An unwrapped first
    // copy stays visible at every width, so a desktop window showed the same
    // project in two identical dropdowns side by side.
    it("never leaves two visible copies: the mobile copy is hidden from md up and the desktop copy below it", async () => {
      localStorage.setItem("aiqa.orchestrator.apiKey", RAW_KEY);
      mockBackend({ projects: [PROJECT_A] });
      renderApp();
      await verifyKey();

      await waitFor(() => expect(document.getElementById("project-selector-button")).toBeTruthy());

      // The button sits several divs deep inside `ProjectSelector`, so the
      // breakpoint wrapper is somewhere up its ancestor chain, not on the
      // nearest one.
      const ancestorClasses = Array.from(
        document.querySelectorAll("#project-selector-button"),
      ).map((button) => {
        const classes: string[] = [];
        for (let node = button.parentElement; node; node = node.parentElement) {
          classes.push(node.className ?? "");
        }
        return classes.join(" ");
      });
      expect(ancestorClasses).toHaveLength(2);
      // Complementary, so exactly one is visible at any width. Without the
      // `md:hidden` on the first copy these two checks are what fails.
      expect(ancestorClasses.some((c) => c.includes("md:hidden"))).toBe(true);
      expect(ancestorClasses.some((c) => c.includes("hidden md:block"))).toBe(true);
    });

    it("the environment selector is a different control, not a second project picker", async () => {
      localStorage.setItem("aiqa.orchestrator.apiKey", RAW_KEY);
      mockBackend({ projects: [PROJECT_A] });
      renderApp();
      await verifyKey();

      await waitFor(() => expect(document.getElementById("project-selector-button")).toBeTruthy());
      // DEMO / UAT / LIVE is the environment scope; it must not carry the
      // project selector's id, which is how the two were told apart.
      const environmentButtons = Array.from(document.querySelectorAll("button")).filter(
        (b) => (b.textContent ?? "").includes("DEMO"),
      );
      expect(environmentButtons.length).toBeGreaterThan(0);
      for (const button of environmentButtons) {
        expect(button.getAttribute("id")).not.toBe("project-selector-button");
      }
    });
  });
