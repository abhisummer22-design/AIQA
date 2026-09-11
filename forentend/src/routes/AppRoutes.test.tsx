/**
 * FE-4 targeted tests for the routing/navigation layer. Renders the real
 * `AppShell` (real `AuthProvider`, real `AIQAProvider`, real `Sidebar`/
 * `CommandPalette`, real `AppRoutes`) inside a `MemoryRouter` — the same
 * component tree production renders, minus the real `BrowserRouter` (swapped
 * for a deterministic one; see `App.tsx`'s own doc comment on `AppShell`).
 * `fetch` is mocked only at the transport boundary, exactly like FE-1/2/3.
 */
import React from 'react';
import { act, render, screen, waitFor } from '@testing-library/react';
import { MemoryRouter, useLocation } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import App, { AppShell } from '../App';
import { setApiKey } from '../services/aiqaApi';

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}
function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_test' } };
}

/** Renders the real path into a `data-testid` element so tests can assert
 *  on it without reaching into `MemoryRouter`'s private state. */
const LocationProbe: React.FC = () => {
  const location = useLocation();
  return <div data-testid="location-probe" data-pathname={location.pathname} />;
};

function renderApp(initialEntries: string[] = ['/']) {
  return render(
    <MemoryRouter initialEntries={initialEntries}>
      <LocationProbe />
      <AppShell />
    </MemoryRouter>,
  );
}

function currentPath(): string {
  return screen.getByTestId('location-probe').getAttribute('data-pathname') ?? '';
}

/** Drives the real `AuthContext` from `key_present` to `authenticated` by
 *  clicking the real "Verify Connection" CTA (`KeyPresentState`) — the
 *  same real state transition FE-3's own tests exercise, needed here
 *  because a protected route only renders its real content once the key
 *  has actually been validated (`key_present` alone is not enough — see
 *  `AuthContext.tsx`'s own doc comment). */
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

describe('root and direct-route resolution (items 1-3, 8)', () => {
  it('item 1 — root route resolves to the documented default (dashboard)', () => {
    renderApp(['/']);
    expect(currentPath()).toBe('/dashboard');
  });

  it('item 2 — the /dashboard route renders the Dashboard page', async () => {
    setApiKey('aiqa_routetest0000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    renderApp(['/dashboard']);
    await verifyKey();
    expect(document.getElementById('aiqa-test-case-generator-page')).toBeNull();
    // Dashboard's own real shell (FE-10: always renders regardless of
    // project-selection state — no project is selected in this test's own
    // deliberately-empty `/projects` mock).
    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();
    expect(screen.getAllByText('Dashboard').length).toBeGreaterThan(0);
  });

  it('item 3 — the /test-generator route renders the Test Case Generator page', async () => {
    setApiKey('aiqa_routetest0000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    renderApp(['/test-generator']);
    await verifyKey();
    expect(document.getElementById('aiqa-test-case-generator-page')).not.toBeNull();
  });

  it('item 8 — a direct/refreshed navigation to a valid route (not just "/") loads that exact route, not the default', () => {
    setApiKey('aiqa_routetest0000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    renderApp(['/reports']);
    // A route-resolution check only (no auth-gated content asserted here —
    // items 10-16 cover gating; this proves the *router* itself resolved
    // the deep-linked path, independent of what's shown inside it).
    expect(currentPath()).toBe('/reports');
  });
});

describe('navigation + active state (items 4-5)', () => {
  it('item 4 — clicking a Sidebar nav link resolves to the correct path', async () => {
    setApiKey('aiqa_routetest0000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    renderApp(['/dashboard']);

    await act(async () => {
      document.getElementById('nav-item-generator')!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });
    expect(currentPath()).toBe('/test-generator');
  });

  it('item 5 — the active nav item changes with the real route (not a separate string comparison)', async () => {
    setApiKey('aiqa_routetest0000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    renderApp(['/dashboard']);

    const dashboardBtn = document.getElementById('nav-item-dashboard')!;
    const generatorBtn = document.getElementById('nav-item-generator')!;
    expect(dashboardBtn.className).toContain('bg-violet-600'); // active styling
    expect(generatorBtn.className).not.toContain('bg-violet-600');

    await act(async () => {
      generatorBtn.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    });

    expect(document.getElementById('nav-item-generator')!.className).toContain('bg-violet-600');
    expect(document.getElementById('nav-item-dashboard')!.className).not.toContain('bg-violet-600');
  });
});

describe('browser history (items 6-7)', () => {
  // `MemoryRouter` (used everywhere else in this file) keeps its own
  // isolated in-memory history stack and deliberately does not observe
  // `window.history`/`popstate` at all — the real, default `App` export
  // (real `BrowserRouter`) is used for these two tests specifically, since
  // testing actual browser Back/Forward requires the router that actually
  // listens for `popstate`. jsdom implements the real History API, so
  // `window.history.pushState`/`.back()`/`.forward()` behave as they
  // would in a real browser.
  // `App` already renders its own `<BrowserRouter>` internally (no seam to
  // inject `LocationProbe` inside that same Router context from outside),
  // so these two tests read `window.location.pathname` directly instead —
  // with a *real* `BrowserRouter`, `window.location` genuinely is the
  // source of truth, unlike `MemoryRouter`'s isolated internal stack.
  function renderRealBrowserApp() {
    return render(<App />);
  }

  beforeEach(() => {
    window.history.pushState({}, '', '/');
  });

  it('item 6 — browser Back returns to the previous route', async () => {
    setApiKey('aiqa_routetest0000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    window.history.pushState({}, '', '/dashboard');
    window.history.pushState({}, '', '/test-generator');
    renderRealBrowserApp();
    expect(window.location.pathname).toBe('/test-generator');

    // jsdom dispatches the `popstate` resulting from `back()` on its own
    // task queue, not synchronously within `back()` itself — `waitFor`
    // polls until it lands instead of asserting on a stale `window.location`
    // after a single fixed flush (which proved unreliable).
    await act(async () => {
      window.history.back();
    });
    await waitFor(() => {
      expect(window.location.pathname).toBe('/dashboard');
    });
  });

  it('item 7 — browser Forward re-applies the route that was gone back from', async () => {
    setApiKey('aiqa_routetest0000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    window.history.pushState({}, '', '/dashboard');
    window.history.pushState({}, '', '/test-generator');
    renderRealBrowserApp();

    await act(async () => {
      window.history.back();
    });
    await waitFor(() => {
      expect(window.location.pathname).toBe('/dashboard');
    });

    await act(async () => {
      window.history.forward();
    });
    await waitFor(() => {
      expect(window.location.pathname).toBe('/test-generator');
    });
  });
});

describe('unknown routes (item 9)', () => {
  it('item 9 — an unknown route renders Not Found, not a blank page', () => {
    setApiKey('aiqa_routetest0000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    renderApp(['/this-does-not-exist']);
    expect(screen.getByText('Page not found')).toBeTruthy();
    expect(document.getElementById('main-content-area')?.textContent?.length ?? 0).toBeGreaterThan(0);
  });
});

describe('route protection via the real AuthContext / StateGate (items 10-16)', () => {
  it('item 10 — no_key: a protected route never renders protected content', () => {
    renderApp(['/dashboard']);
    expect(screen.getByText(/NO KEY CONFIGURED/)).toBeTruthy();
    expect(screen.queryByText('Execution Hub')).toBeNull();
  });

  it('item 11 — key_present: a stored key directly renders protected content without manual verification gate', async () => {
    setApiKey('aiqa_unverified00000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    renderApp(['/dashboard']);
    expect(screen.queryByText('KEY SAVED · UNVERIFIED')).toBeNull();
    expect(screen.queryByText('Verify Connection')).toBeNull();
    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();
  });

  it('item 12 — checking: protected content renders directly while background validation is in flight', async () => {
    setApiKey('aiqa_checking00000000000');
    let resolveFetch: (r: Response) => void;
    vi.stubGlobal(
      'fetch',
      vi.fn().mockReturnValue(
        new Promise<Response>((resolve) => {
          resolveFetch = resolve;
        }),
      ),
    );
    renderApp(['/dashboard']);

    expect(screen.queryByText('VERIFYING…')).toBeNull();
    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();

    await act(async () => {
      resolveFetch!(jsonResponse(200, []));
    });
  });

  it('item 13 — authenticated: the protected route renders its real content', async () => {
    setApiKey('aiqa_ok00000000000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    renderApp(['/dashboard']);

    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();
    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
  });

  it('item 14 — unauthenticated (401): shows the 401 state once backend rejects with 401', async () => {
    setApiKey('aiqa_bad0000000000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(401, envelope('UNAUTHENTICATED', 'invalid key'))));
    renderApp(['/dashboard']);

    await waitFor(() => expect(screen.getByText('AUTHENTICATION REQUIRED · 401')).toBeTruthy());
    expect(screen.queryByText('Execution Hub')).toBeNull();
  });

  it('item 15 — forbidden (403): preserved on protected content', async () => {
    setApiKey('aiqa_scoped0000000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(403, envelope('FORBIDDEN', 'wrong scope'))));
    renderApp(['/dashboard']);

    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();
  });

  it('item 16 — offline: renders protected content directly even if backend is offline on startup', async () => {
    setApiKey('aiqa_offline000000000000');
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')));
    renderApp(['/dashboard']);

    expect(screen.queryByText('Key saved, not yet verified')).toBeNull();
    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();
  });

  it('Settings remains reachable with no key (deliberately not gated — see routeConfig.tsx)', async () => {
    renderApp(['/settings']);
    // The real credential panel lives under Settings' own pre-existing
    // "Authentication & SSO" tab (defaults to "General" — this click is
    // exercising that existing, unmodified tab UI, not new FE-4 behavior).
    await act(async () => {
      screen.getByText('Authentication & SSO').closest('button')!.click();
    });
    expect(screen.getByText('AIQA Orchestrator API Key')).toBeTruthy();
  });
});

describe('security (item 17)', () => {
  it('item 17 — no API key ever appears in the URL, in any state', async () => {
    const raw = 'aiqa_neverinurl00000000';
    setApiKey(raw);
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    renderApp(['/dashboard']);
    await verifyKey();

    expect(currentPath()).not.toContain(raw);
    expect(window.location.search).not.toContain(raw);
  });
});

describe('existing page regression (items 18-19)', () => {
  it('item 18 — Dashboard: the existing product-owner-designed UI renders, now with real (not fabricated) project-scoped data', async () => {
    setApiKey('aiqa_ok00000000000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    renderApp(['/dashboard']);
    await verifyKey();
    expect(screen.getAllByText('Dashboard').length).toBeGreaterThan(0);
    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();
    await waitFor(() => expect(screen.getByText('No project selected')).toBeTruthy());
    expect(screen.queryByText('AIQA Platform')).toBeNull();
  });

  it('item 19 — Test Generator: the existing product-owner-designed UI renders unregressed', async () => {
    setApiKey('aiqa_ok00000000000000000');
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(jsonResponse(200, [])));
    renderApp(['/test-generator']);
    await verifyKey();
    expect(document.getElementById('aiqa-test-case-generator-page')).not.toBeNull();
  });
});
