/**
 * Dedicated tests for `MobileTestingPage`.
 *
 * The page stopped being a deferred placeholder once real device discovery
 * existed (`mobile.rs` → `GET /api/v1/mobile/fleet`). The invariants worth
 * protecting are the ones the page's *original* fabricated fleet violated:
 * no invented devices, an empty host reported as genuinely empty, adb's own
 * state strings shown verbatim (an `unauthorized` handset never presented as
 * usable), and no dispatch affordance offered when the real toolchain or a
 * real device is missing.
 */
import React from 'react';
import { act, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { MobileFleet, Project } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}
function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_mobile_test' } };
}

const RAW_KEY = 'aiqa_mobiletest000000000';
const PROJECT_A: Project = {
  id: 'proj-mobile',
  name: 'Mobile Project',
  description: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};

/** A real-shaped fleet matching the live backend response observed from
 *  this host (`mobile.rs::MobileFleet`). */
const FLEET: MobileFleet = {
  devices: [],
  avds: [{ name: 'e2e_test', running: false }],
  adb: { path: 'C:\\Android\\Sdk\\platform-tools\\adb.exe', available: true, detail: 'adb reachable' },
  emulator_tool: { path: 'C:\\Android\\Sdk\\emulator\\emulator.exe', available: true, detail: 'emulator reachable' },
  maestro: { path: 'C:\\Users\\x\\.maestro\\maestro\\bin\\maestro.bat', available: true, detail: 'Maestro 2.8.0' },
  ios_devices: [],
  simctl: {
    path: null,
    available: false,
    detail: 'iOS Simulators require macOS - Apple ships no Simulator runtime for windows. This is a platform limit, not a missing install.',
  },
  libimobiledevice: {
    path: null,
    available: false,
    detail: 'libimobiledevice not installed - required to see a physically attached iPhone/iPad',
  },
  host_os: 'windows',
};

function mockBackend(fleet: unknown = FLEET, onBoot?: () => Response) {
  const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
    const u = String(url);
    if (u.includes('/mobile/emulators/boot') && init?.method === 'POST') {
      return onBoot ? onBoot() : jsonResponse(200, { message: "emulator 'e2e_test' starting (pid 4242)" });
    }
    if (u.includes('/mobile/fleet')) return jsonResponse(200, fleet);
    if (u.includes('/logs')) return jsonResponse(200, []);
    return jsonResponse(200, [PROJECT_A]);
  });
  vi.stubGlobal('fetch', fetchMock);
  return fetchMock;
}

function renderApp(path = '/mobile-testing') {
  return render(
    <MemoryRouter initialEntries={[path]}>
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

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('MobileTestingPage — real device fleet', () => {
  it('unauthenticated (no key) shows the real auth-state card and asks the host for nothing', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    renderApp();

    await waitFor(() => expect(screen.getByText('No orchestrator API key configured')).toBeTruthy());
    expect(fetchMock).not.toHaveBeenCalled();
    expect(document.getElementById('mobile-testing-page')).toBeNull();
  });

  it('reads the REAL fleet endpoint and renders the real AVDs that exist on this host', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = mockBackend();
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText('e2e_test')).toBeTruthy());
    const urls = (fetchMock.mock.calls as [string][]).map(([u]) => String(u));
    expect(urls.some((u) => u.includes('/api/v1/mobile/fleet'))).toBe(true);
  });

  it('an empty host is reported as a genuinely empty fleet — no invented devices', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText(/No device or emulator is currently connected/)).toBeTruthy());
    // None of the page's original fabricated fleet survives.
    for (const banned of ['Samsung Galaxy S24 Ultra', 'DEV-AND-01', 'Lock & Reserve Device', 'Execute Non-UI Suite']) {
      expect(screen.queryByText(new RegExp(banned.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')))).toBeNull();
    }
  });

  it("renders adb's own state string verbatim and never marks an unauthorized handset usable", async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      ...FLEET,
      devices: [
        { serial: 'R58M12ABCDE', state: 'unauthorized', model: null, ready: false, emulator: false },
        { serial: 'emulator-5554', state: 'device', model: 'sdk_gphone64_x86_64', ready: true, emulator: true },
      ],
    });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText('R58M12ABCDE')).toBeTruthy());
    const unauth = document.getElementById('mobile-device-R58M12ABCDE')!;
    expect(within(unauth).getByText('unauthorized')).toBeTruthy();
    expect(within(unauth).getByText(/Accept the USB debugging prompt/)).toBeTruthy();
    // Real physical/emulator distinction, from the real serial convention.
    expect(within(unauth).getByText('physical')).toBeTruthy();
    const emu = document.getElementById('mobile-device-emulator-5554')!;
    expect(within(emu).getByText('emulator')).toBeTruthy();
    expect(within(emu).getByText('device')).toBeTruthy();
  });

  it('booting an AVD makes a REAL request and never claims the emulator is already up', async () => {
    setApiKey(RAW_KEY);
    let bootUrl: string | null = null;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      const u = String(url);
      if (u.includes('/mobile/emulators/boot') && init?.method === 'POST') {
        bootUrl = u;
        expect(JSON.parse(String(init.body))).toEqual({ avd: 'e2e_test' });
        return jsonResponse(200, { message: "emulator 'e2e_test' starting (pid 4242)" });
      }
      if (u.includes('/mobile/fleet')) return jsonResponse(200, FLEET);
      if (u.includes('/logs')) return jsonResponse(200, []);
      return jsonResponse(200, [PROJECT_A]);
    });
    vi.stubGlobal('fetch', fetchMock);

    renderApp();
    await verifyKey();
    await waitFor(() => expect(document.getElementById('btn-boot-avd-e2e_test')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-boot-avd-e2e_test')!.click();
    });

    await waitFor(() => expect(screen.getByText(/starting \(pid 4242\)/)).toBeTruthy());
    expect(String(bootUrl)).toMatch(/\/api\/v1\/mobile\/emulators\/boot$/);
    // Only adb can prove readiness — the page must not assert it booted.
    expect(screen.getByText(/will appear above as a real device once adb sees it/)).toBeTruthy();
  });

  it('a real boot failure is surfaced verbatim — never a fabricated success', async () => {
    setApiKey(RAW_KEY);
    mockBackend(FLEET, () => jsonResponse(400, envelope('VALIDATION_ERROR', 'could not start emulator: no such AVD')));
    renderApp();
    await verifyKey();
    await waitFor(() => expect(document.getElementById('btn-boot-avd-e2e_test')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-boot-avd-e2e_test')!.click();
    });

    await waitFor(() => expect(screen.getByText(/could not start emulator: no such AVD/)).toBeTruthy());
    expect(screen.queryByText(/starting \(pid/)).toBeNull();
  });

  it('renders the real toolchain with real versions', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText('Maestro 2.8.0')).toBeTruthy());
    expect(screen.getByText('adb reachable')).toBeTruthy();
    expect(within(document.getElementById('mobile-tool-maestro')!).getByText('available')).toBeTruthy();
  });

  it('offers no dispatch when Maestro is genuinely missing, and says why', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      ...FLEET,
      devices: [{ serial: 'emulator-5554', state: 'device', model: 'sdk', ready: true, emulator: true }],
      maestro: { path: null, available: false, detail: 'maestro not found' },
    });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText(/Dispatch is unavailable: maestro not found/)).toBeTruthy());
    expect((document.getElementById('btn-mobile-open-execution-hub') as HTMLButtonElement).disabled).toBe(true);
  });

  it('offers no dispatch when no device is in the adb `device` state', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      ...FLEET,
      devices: [{ serial: 'R58M12ABCDE', state: 'unauthorized', model: null, ready: false, emulator: false }],
    });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText(/no Android device is in the adb `device` state and no iOS device is ready/)).toBeTruthy());
    expect((document.getElementById('btn-mobile-open-execution-hub') as HTMLButtonElement).disabled).toBe(true);
  });

  it('enables dispatch only when Maestro AND a ready device both genuinely exist, and discloses the policy gate', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      ...FLEET,
      devices: [{ serial: 'emulator-5554', state: 'device', model: 'sdk', ready: true, emulator: true }],
    });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText(/Maestro is installed and 1 device is ready/)).toBeTruthy());
    expect((document.getElementById('btn-mobile-open-execution-hub') as HTMLButtonElement).disabled).toBe(false);
    // The real policy gate on the maestro executor is disclosed, not hidden.
    expect(screen.getByText(/PENDING_APPROVAL/)).toBeTruthy();
  });

  it('a real fleet-read failure replaces the page content outright — no stale or invented fleet', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        const u = String(url);
        if (u.includes('/mobile/fleet')) return jsonResponse(500, envelope('INTERNAL', 'boom'));
        if (u.includes('/logs')) return jsonResponse(200, []);
        return jsonResponse(200, [PROJECT_A]);
      }),
    );
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText("Couldn't read the device fleet")).toBeTruthy());
    expect(screen.queryByText('e2e_test')).toBeNull();
  });

  it('states the real macOS platform limit for iOS Simulators instead of showing an empty fleet', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp();
    await verifyKey();

    await waitFor(() => expect(document.getElementById('ios-unavailable-notice')).toBeTruthy());
    const notice = document.getElementById('ios-unavailable-notice')!;
    // A platform limit must read as a platform limit, not as "none created yet".
    // (The same real detail also appears in the toolchain row below, which is
    // why this is scoped to the notice rather than searched page-wide.)
    expect(within(notice).getByText(/iOS Simulators require macOS/)).toBeTruthy();
    // And the separate, genuinely installable path is named as such.
    expect(within(notice).getByText(/libimobiledevice not installed/)).toBeTruthy();
    // No invented iPhone.
    expect(screen.queryByText(/iPhone 15/)).toBeNull();
  });

  it('renders REAL iOS simulators with their real state and runtime when the host has them', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      ...FLEET,
      simctl: { path: 'xcrun', available: true, detail: 'simctl reachable - 2 available simulator(s)' },
      host_os: 'macos',
      ios_devices: [
        {
          udid: 'A1B2C3D4-0000-1111-2222-333344445555',
          name: 'iPhone 15 Pro',
          runtime: 'iOS 17 5',
          state: 'Booted',
          ready: true,
          simulator: true,
        },
        {
          udid: 'B1B2C3D4-0000-1111-2222-333344445555',
          name: 'iPad Air 11-inch',
          runtime: 'iOS 17 5',
          state: 'Shutdown',
          ready: false,
          simulator: true,
        },
      ],
    });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText('iPhone 15 Pro')).toBeTruthy());
    const booted = document.getElementById('ios-device-A1B2C3D4-0000-1111-2222-333344445555')!;
    expect(within(booted).getByText('Booted')).toBeTruthy();
    expect(within(booted).getByText('simulator')).toBeTruthy();
    // A Booted simulator needs no Boot button; a Shutdown one gets a real button.
    expect(document.getElementById('btn-boot-sim-A1B2C3D4-0000-1111-2222-333344445555')).toBeNull();
    expect(document.getElementById('btn-boot-sim-B1B2C3D4-0000-1111-2222-333344445555')).toBeTruthy();
  });

  it('a physically attached iPhone is shown as physical hardware, not a simulator', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      ...FLEET,
      libimobiledevice: { path: '/usr/bin/idevice_id', available: true, detail: 'libimobiledevice reachable - 1 attached device(s)' },
      ios_devices: [
        {
          udid: '00008110-000A1B2C3D4E801E',
          name: "Abhi's iPhone",
          runtime: 'iOS 18.2',
          state: 'Connected',
          ready: true,
          simulator: false,
        },
      ],
    });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText("Abhi's iPhone")).toBeTruthy());
    const dev = document.getElementById('ios-device-00008110-000A1B2C3D4E801E')!;
    expect(within(dev).getByText('physical')).toBeTruthy();
    expect(within(dev).getByText('Connected')).toBeTruthy();
    // Real hardware is never offered a simulator boot control.
    expect(document.getElementById('btn-boot-sim-00008110-000A1B2C3D4E801E')).toBeNull();
  });

  it('booting an iOS simulator makes a REAL request to the real endpoint', async () => {
    setApiKey(RAW_KEY);
    let bootBody: unknown = null;
    const iosFleet = {
      ...FLEET,
      simctl: { path: 'xcrun', available: true, detail: 'simctl reachable - 1 available simulator(s)' },
      host_os: 'macos',
      ios_devices: [
        {
          udid: 'B1B2C3D4-0000-1111-2222-333344445555',
          name: 'iPad Air 11-inch',
          runtime: 'iOS 17 5',
          state: 'Shutdown',
          ready: false,
          simulator: true,
        },
      ],
    };
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        const u = String(url);
        if (u.includes('/mobile/simulators/boot') && init?.method === 'POST') {
          bootBody = JSON.parse(String(init.body));
          return jsonResponse(200, { message: 'simulator B1B2C3D4-0000-1111-2222-333344445555 booting' });
        }
        if (u.includes('/mobile/fleet')) return jsonResponse(200, iosFleet);
        if (u.includes('/logs')) return jsonResponse(200, []);
        return jsonResponse(200, [PROJECT_A]);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(document.getElementById('btn-boot-sim-B1B2C3D4-0000-1111-2222-333344445555')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-boot-sim-B1B2C3D4-0000-1111-2222-333344445555')!.click();
    });

    await waitFor(() => expect(screen.getByText(/simulator B1B2C3D4-0000-1111-2222-333344445555 booting/)).toBeTruthy());
    expect(bootBody).toEqual({ udid: 'B1B2C3D4-0000-1111-2222-333344445555' });
  });

  it('a real iOS boot refusal is surfaced verbatim, including the platform reason', async () => {
    setApiKey(RAW_KEY);
    const iosFleet = {
      ...FLEET,
      ios_devices: [
        {
          udid: 'B1B2C3D4-0000-1111-2222-333344445555',
          name: 'iPad Air 11-inch',
          runtime: 'iOS 17 5',
          state: 'Shutdown',
          ready: false,
          simulator: true,
        },
      ],
    };
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        const u = String(url);
        if (u.includes('/mobile/simulators/boot') && init?.method === 'POST') {
          return jsonResponse(
            400,
            envelope('VALIDATION_ERROR', 'iOS Simulators require macOS - cannot boot one on windows'),
          );
        }
        if (u.includes('/mobile/fleet')) return jsonResponse(200, iosFleet);
        if (u.includes('/logs')) return jsonResponse(200, []);
        return jsonResponse(200, [PROJECT_A]);
      }),
    );

    renderApp();
    await verifyKey();
    await waitFor(() => expect(document.getElementById('btn-boot-sim-B1B2C3D4-0000-1111-2222-333344445555')).toBeTruthy());

    await act(async () => {
      document.getElementById('btn-boot-sim-B1B2C3D4-0000-1111-2222-333344445555')!.click();
    });

    await waitFor(() => expect(screen.getByText(/cannot boot one on windows/)).toBeTruthy());
  });

  it('a ready iOS device alone enables dispatch — Maestro drives both platforms', async () => {
    setApiKey(RAW_KEY);
    mockBackend({
      ...FLEET,
      devices: [],
      ios_devices: [
        {
          udid: 'A1B2C3D4-0000-1111-2222-333344445555',
          name: 'iPhone 15 Pro',
          runtime: 'iOS 17 5',
          state: 'Booted',
          ready: true,
          simulator: true,
        },
      ],
    });
    renderApp();
    await verifyKey();

    await waitFor(() => expect(screen.getByText(/Maestro is installed and 1 device is ready/)).toBeTruthy());
    expect((document.getElementById('btn-mobile-open-execution-hub') as HTMLButtonElement).disabled).toBe(false);
  });

  it('Dashboard remains unregressed with the real MobileTestingPage in the route table', async () => {
    setApiKey(RAW_KEY);
    mockBackend();
    renderApp('/dashboard');
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy());
  });
});
