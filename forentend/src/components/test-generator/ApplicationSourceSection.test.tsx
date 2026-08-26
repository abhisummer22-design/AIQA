/**
 * Truthfulness Cleanup — Step 1 (Application Source connectivity).
 *
 * Renders `ApplicationSourceSection` directly (not the full `AppShell`) —
 * this is a focused, standalone component test of its own real state
 * machine (`IDLE`/`CHECKING`/`CONNECTED`/`UNREACHABLE`/`REJECTED`/
 * `INVALID_URL`/`ERROR`), all driven by the real `aiqaApi.appSource.verify`
 * client call. `fetch` is mocked only at the transport boundary, same
 * convention as every other test in this app — never the `aiqaApi` module
 * itself, so the real request-building/auth-header logic is still exercised.
 */
import React from 'react';
import { act, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { ApplicationSourceSection } from './ApplicationSourceSection';
import { setApiKey } from '../../services/aiqaApi';
import type { AppSourceConnection, AppSourceType } from '../../types';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const RAW_KEY = 'aiqa_appsource00000000000000';
const PROJECT_ID = 'proj-app-source-1';

function initialSources(): Record<AppSourceType, AppSourceConnection> {
  return {
    STAGING: { type: 'STAGING', label: 'Staging', url: '', status: 'IDLE' },
    DEMO: { type: 'DEMO', label: 'Demo', url: '', status: 'IDLE' },
    LIVE: { type: 'LIVE', label: 'Live', url: '', status: 'IDLE' },
    FIGMA: { type: 'FIGMA', label: 'Figma', url: '', status: 'IDLE' },
  };
}

function Harness({ initial, projectId = PROJECT_ID }: { initial?: Record<AppSourceType, AppSourceConnection>; projectId?: string | null }) {
  const [sources, setSources] = React.useState<Record<AppSourceType, AppSourceConnection>>(initial ?? initialSources());
  return <ApplicationSourceSection appSources={sources} onAppSourcesChange={setSources} projectId={projectId} />;
}

function stagingInput(): HTMLInputElement {
  return document.querySelector('#source-card-staging input') as HTMLInputElement;
}
function stagingConnectButton(): HTMLButtonElement {
  return document.querySelector('#source-card-staging button') as HTMLButtonElement;
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('ApplicationSourceSection — initial state is truthful', () => {
  it('shows Not Connected, never a fake Connected/HTTP 200/SSL Valid before any check is run', () => {
    render(<Harness />);
    expect(screen.getAllByText('Not Connected').length).toBeGreaterThan(0);
    expect(screen.queryByText('Connected')).toBeNull();
    expect(screen.queryByText(/HTTP 200/)).toBeNull();
    expect(screen.queryByText(/SSL/)).toBeNull();
    expect(screen.queryByText(/Ping/i)).toBeNull();
  });
});

describe('ApplicationSourceSection — real verification state machine', () => {
  it('shows Checking while the real request is in flight, driven by a real pending fetch — not a fixed delay', async () => {
    let resolveFetch: (r: Response) => void = () => {};
    const pending = new Promise<Response>((resolve) => {
      resolveFetch = resolve;
    });
    const fetchMock = vi.fn().mockImplementation(async () => pending);
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    fireEvent.change(stagingInput(), { target: { value: 'https://staging.example.com' } });
    await act(async () => {
      fireEvent.click(stagingConnectButton());
    });

    expect(screen.getByText('Checking')).toBeTruthy();
    expect((stagingConnectButton() as HTMLButtonElement).disabled).toBe(true);

    await act(async () => {
      resolveFetch(jsonResponse(200, { status: 'CONNECTED', reachable: true, http_status: 200, latency_ms: 42, message: 'Reachable — real HTTP 200 OK in 42ms.' }));
      await pending;
    });
    expect(screen.getByText('Connected')).toBeTruthy();
  });

  it('CONNECTED renders only the real returned HTTP status and real measured latency — no random/fabricated metric', async () => {
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      expect(url).toContain('/verify-application-source');
      expect(JSON.parse(String(init?.body))).toEqual({ url: 'https://staging.example.com' });
      return jsonResponse(200, { status: 'CONNECTED', reachable: true, http_status: 200, latency_ms: 57, message: 'Reachable — real HTTP 200 OK in 57ms.' });
    });
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    fireEvent.change(stagingInput(), { target: { value: 'https://staging.example.com' } });
    await act(async () => {
      fireEvent.click(stagingConnectButton());
    });

    expect(screen.getByText('Connected')).toBeTruthy();
    expect(screen.getByText(/HTTP 200/)).toBeTruthy();
    expect(screen.getByText(/57ms/)).toBeTruthy();
  });

  it('a real reachable non-2xx response is still shown as Connected with the real status', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, { status: 'CONNECTED', reachable: true, http_status: 404, latency_ms: 12, message: 'Reachable — real HTTP 404 Not Found in 12ms.' }));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    fireEvent.change(stagingInput(), { target: { value: 'https://staging.example.com' } });
    await act(async () => {
      fireEvent.click(stagingConnectButton());
    });

    expect(screen.getByText('Connected')).toBeTruthy();
    expect(screen.getByText(/HTTP 404/)).toBeTruthy();
  });

  it('UNREACHABLE renders the real truthful state, never as Connected', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, { status: 'UNREACHABLE', reachable: false, http_status: null, latency_ms: 3012, message: 'The connection attempt timed out.' }));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    fireEvent.change(stagingInput(), { target: { value: 'https://staging.example.com' } });
    await act(async () => {
      fireEvent.click(stagingConnectButton());
    });

    expect(screen.getByText('Unreachable')).toBeTruthy();
    expect(screen.getByText('The connection attempt timed out.')).toBeTruthy();
    expect(screen.queryByText('Connected')).toBeNull();
  });

  it('REJECTED renders the real security-policy state, without exposing internal detail', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, { status: 'REJECTED', reachable: false, http_status: null, latency_ms: null, message: 'This target is not allowed by the connection security policy.' }));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    fireEvent.change(stagingInput(), { target: { value: 'http://169.254.169.254/' } });
    await act(async () => {
      fireEvent.click(stagingConnectButton());
    });

    expect(screen.getByText('Rejected')).toBeTruthy();
    expect(screen.getByText('This target is not allowed by the connection security policy.')).toBeTruthy();
  });

  it('a real backend INVALID_URL response renders Invalid URL', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, { status: 'INVALID_URL', reachable: false, http_status: null, latency_ms: null, message: 'Only http and https URLs are supported.' }));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    // Bypass the client-side format pre-check by supplying a syntactically
    // valid URL, so the request genuinely reaches the backend and this test
    // proves the backend's own INVALID_URL classification renders correctly.
    fireEvent.change(stagingInput(), { target: { value: 'https://valid-looking-but-backend-rejects.example' } });
    await act(async () => {
      fireEvent.click(stagingConnectButton());
    });

    expect(screen.getByText('Invalid URL')).toBeTruthy();
  });

  it('a malformed URL is rejected client-side immediately, with no backend request', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    fireEvent.change(stagingInput(), { target: { value: 'not a url' } });
    await act(async () => {
      fireEvent.click(stagingConnectButton());
    });

    expect(screen.getByText('Invalid URL')).toBeTruthy();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('ERROR renders when the verification request itself fails, never silently becoming Connected', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(500, { error: { code: 'INTERNAL_ERROR', message: 'unexpected failure', details: {}, request_id: 'req_1' } }));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    fireEvent.change(stagingInput(), { target: { value: 'https://staging.example.com' } });
    await act(async () => {
      fireEvent.click(stagingConnectButton());
    });

    expect(screen.getByText('Error')).toBeTruthy();
    expect(screen.queryByText('Connected')).toBeNull();
  });

  it('no project selected disables Connect with a truthful reason — no request is sent', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness projectId={null} />);

    expect(screen.getByText(/Select a project before verifying a connection/)).toBeTruthy();
    fireEvent.change(stagingInput(), { target: { value: 'https://staging.example.com' } });
    expect(stagingConnectButton().disabled).toBe(true);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('Disconnect resets to the real Idle state, not a fabricated one', async () => {
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, { status: 'CONNECTED', reachable: true, http_status: 200, latency_ms: 20, message: 'Reachable — real HTTP 200 OK in 20ms.' }));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey(RAW_KEY);
    render(<Harness />);

    fireEvent.change(stagingInput(), { target: { value: 'https://staging.example.com' } });
    await act(async () => {
      fireEvent.click(stagingConnectButton());
    });
    expect(screen.getByText('Connected')).toBeTruthy();

    await act(async () => {
      fireEvent.click(screen.getByText('Disconnect'));
    });
    expect(screen.getAllByText('Not Connected').length).toBeGreaterThan(0);
    expect(screen.queryByText('Connected')).toBeNull();
  });
});
