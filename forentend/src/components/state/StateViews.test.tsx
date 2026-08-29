/**
 * FE-3 targeted tests for the state-view layer.
 *
 * Two kinds of coverage, both required by this sprint's "critical rule"
 * (state views must be driven by the real `AuthContext` state machine, not
 * static mock screens):
 *
 *  - Most tests construct a real `AuthContextType` value (the same shape
 *    `AuthProvider` produces) and pass it straight to `renderAuthState`/
 *    `StateGate` — this is the standard "render given state" pattern for
 *    a pure view layer, not a static mock: every value used is a real,
 *    reachable value of the real context type.
 *  - The "driven by the real state machine" tests at the bottom go
 *    further: they mount the real `AuthProvider`, mock only `fetch`, and
 *    drive real transitions (save → checking → authenticated/401/403/
 *    offline) through `useAuth()`, asserting the rendered output tracks
 *    the real context end to end.
 */
import React from 'react';
import { act, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { renderAuthState, StateGate } from './StateViews';
import { ApiError } from '../../services/aiqaApi';
import type { AuthContextType, AuthStatus } from '../../context/AuthContext';
import { AuthProvider, useAuth } from '../../context/AuthContext';

function makeAuth(overrides: Partial<AuthContextType> & { status: AuthStatus }): AuthContextType {
  return {
    hasKey: overrides.status !== 'no_key',
    maskedKey: null,
    lastError: null,
    saveKey: vi.fn().mockResolvedValue(undefined),
    validate: vi.fn().mockResolvedValue(undefined),
    signOut: vi.fn(),
    ...overrides,
  };
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('renderAuthState — one card per real AuthStatus (items 1-8)', () => {
  it('item 1 — no_key renders the No Key state', () => {
    render(<>{renderAuthState(makeAuth({ status: 'no_key' }))}</>);
    expect(screen.getByText(/NO KEY CONFIGURED/i)).toBeTruthy();
    expect(screen.getByText(/Configure an AIQA Orchestrator API key/i)).toBeTruthy();
  });

  it('item 2 — key_present renders masked/unverified state', () => {
    render(<>{renderAuthState(makeAuth({ status: 'key_present', maskedKey: 'aiqa_a…••••••' }))}</>);
    expect(screen.getByText(/KEY SAVED · UNVERIFIED/i)).toBeTruthy();
    expect(screen.getByText('aiqa_a…••••••')).toBeTruthy();
  });

  it('item 3 — checking renders a loading state', () => {
    render(<>{renderAuthState(makeAuth({ status: 'checking', maskedKey: 'aiqa_a…••••••' }))}</>);
    // Exact-case match: the badge label ("VERIFYING…") and the title
    // ("Verifying connection") both loosely match a case-insensitive
    // /VERIFYING/i, so this targets the badge specifically.
    expect(screen.getByText('VERIFYING…')).toBeTruthy();
    expect(screen.getByText(/Checking the configured API key/i)).toBeTruthy();
  });

  it('item 4 — authenticated renders a connected state', () => {
    render(<>{renderAuthState(makeAuth({ status: 'authenticated', maskedKey: 'aiqa_a…••••••' }))}</>);
    expect(screen.getByText(/CONNECTED/i)).toBeTruthy();
    expect(screen.getByText(/authentication verified/i)).toBeTruthy();
  });

  it('item 5 — unauthenticated renders the 401-specific state', () => {
    render(<>{renderAuthState(makeAuth({ status: 'unauthenticated', maskedKey: 'aiqa_a…••••••' }))}</>);
    expect(screen.getByText(/AUTHENTICATION REQUIRED · 401/i)).toBeTruthy();
  });

  it('item 6 — forbidden renders the 403-specific state', () => {
    render(<>{renderAuthState(makeAuth({ status: 'forbidden', maskedKey: 'aiqa_a…••••••' }))}</>);
    expect(screen.getByText(/ACCESS FORBIDDEN · 403/i)).toBeTruthy();
  });

  it('item 7 — offline renders the offline state, not an auth failure', () => {
    render(<>{renderAuthState(makeAuth({ status: 'offline', maskedKey: 'aiqa_a…••••••' }))}</>);
    // Exact-case match: the badge ("ORCHESTRATOR UNREACHABLE") and the
    // title ("Orchestrator unreachable") differ only in case, which a
    // case-insensitive regex would match on both.
    expect(screen.getByText('ORCHESTRATOR UNREACHABLE')).toBeTruthy();
    expect(screen.queryByText(/401/)).toBeNull();
    expect(screen.queryByText(/403/)).toBeNull();
  });

  it('item 8 — error renders a generic recoverable error state with a safe message and request id', () => {
    const err = new ApiError('server_error', 'The orchestrator returned an internal error.', 500, 'INTERNAL_ERROR', null, 'req_abc123');
    render(<>{renderAuthState(makeAuth({ status: 'error', maskedKey: 'aiqa_a…••••••', lastError: err }))}</>);
    expect(screen.getByText('The orchestrator returned an internal error.')).toBeTruthy();
    expect(screen.getByText(/req_abc123/)).toBeTruthy();
  });
});

describe('401 vs 403 distinctness and security (items 9-13)', () => {
  it('item 9 — 401 and 403 are visibly and semantically distinct', () => {
    const { unmount } = render(<>{renderAuthState(makeAuth({ status: 'unauthenticated' }))}</>);
    const status401 = screen.getByText(/AUTHENTICATION REQUIRED · 401/i);
    const desc401 = screen.getByText(/It may be invalid, revoked, or mistyped/i);
    unmount();

    render(<>{renderAuthState(makeAuth({ status: 'forbidden' }))}</>);
    const status403 = screen.getByText(/ACCESS FORBIDDEN · 403/i);
    const desc403 = screen.getByText(/permission\/scope problem, not a bad credential/i);

    // Different label text (not just color) and different explanatory copy.
    expect(status401.textContent).not.toBe(status403.textContent);
    expect(desc401.textContent).not.toBe(desc403.textContent);
    // 403's own card never uses 401's wording, and vice versa (already
    // implied by the two renders above using disjoint `getByText` regexes,
    // asserted explicitly here too).
    expect(screen.queryByText(/AUTHENTICATION REQUIRED · 401/i)).toBeNull();
  });

  it('item 10 — the raw API key never appears in rendered output, in any state', () => {
    const raw = 'aiqa_rawvalueshouldneverrender0000';
    const masked = 'aiqa_r…••••••';
    const statuses: AuthStatus[] = ['no_key', 'key_present', 'checking', 'authenticated', 'unauthenticated', 'forbidden', 'offline', 'error'];
    for (const status of statuses) {
      const { container, unmount } = render(<>{renderAuthState(makeAuth({ status, maskedKey: masked }))}</>);
      expect(container.innerHTML).not.toContain(raw);
      expect(container.innerHTML.toLowerCase()).not.toContain('authorization');
      expect(container.innerHTML.toLowerCase()).not.toContain('bearer ');
      unmount();
    }
  });

  it('item 11 — 401 state still displays the masked key (it is not cleared)', () => {
    render(<>{renderAuthState(makeAuth({ status: 'unauthenticated', maskedKey: 'aiqa_a…••••••' }))}</>);
    expect(screen.getByText('aiqa_a…••••••')).toBeTruthy();
  });

  it('item 12 — 403 state still displays the masked key (it is not cleared)', () => {
    render(<>{renderAuthState(makeAuth({ status: 'forbidden', maskedKey: 'aiqa_a…••••••' }))}</>);
    expect(screen.getByText('aiqa_a…••••••')).toBeTruthy();
  });

  it('item 13 — offline state still displays the masked key (it is not cleared)', () => {
    render(<>{renderAuthState(makeAuth({ status: 'offline', maskedKey: 'aiqa_a…••••••' }))}</>);
    expect(screen.getByText('aiqa_a…••••••')).toBeTruthy();
  });
});

describe('no automatic retry, manual actions (items 14-15)', () => {
  it('item 14 — rendering any state triggers zero calls to validate/saveKey (no automatic retry on render)', () => {
    const auth = makeAuth({ status: 'unauthenticated', maskedKey: 'aiqa_a…••••••' });
    render(<>{renderAuthState(auth)}</>);
    expect(auth.validate).not.toHaveBeenCalled();
    expect(auth.saveKey).not.toHaveBeenCalled();
  });

  it('item 14b — the checking state renders no actionable retry control at all (eliminates the duplicate-request race by construction)', () => {
    const { container } = render(<>{renderAuthState(makeAuth({ status: 'checking' }))}</>);
    expect(container.querySelectorAll('button').length).toBe(0);
  });

  it('item 15 — "Verify Connection" is present and manually actionable in the key_present state', () => {
    const auth = makeAuth({ status: 'key_present', maskedKey: 'aiqa_a…••••••' });
    render(<>{renderAuthState(auth)}</>);
    const verifyBtn = screen.getByText('Verify Connection').closest('button')!;
    expect(verifyBtn).toBeTruthy();
    expect(verifyBtn.getAttribute('type')).toBe('button');
    verifyBtn.click();
    expect(auth.validate).toHaveBeenCalledTimes(1);
  });

  it('bonus regression guard — Sign Out is reachable and actionable from every state where a key exists, not only "authenticated"', () => {
    // Regression guard: an earlier draft of this component only offered
    // Sign Out from the `authenticated` card, silently stranding a user
    // stuck in 401/403/offline/error/key_present with no way to clear a
    // stuck credential from the state card itself (found during this
    // sprint's own live manual verification, §9 of the report).
    const statusesWithKey: AuthStatus[] = ['key_present', 'authenticated', 'unauthenticated', 'forbidden', 'offline', 'error'];
    for (const status of statusesWithKey) {
      const auth = makeAuth({ status, maskedKey: 'aiqa_a…••••••' });
      const { unmount } = render(<>{renderAuthState(auth)}</>);
      const signOutBtn = screen.getByText('Sign Out').closest('button')!;
      expect(signOutBtn, `Sign Out missing for status=${status}`).toBeTruthy();
      signOutBtn.click();
      expect(auth.signOut, `Sign Out did not call auth.signOut for status=${status}`).toHaveBeenCalledTimes(1);
      unmount();
    }
  });
});

describe('StateGate (reusable gate for future real pages)', () => {
  it('renders children when key is configured and not unauthenticated', () => {
    const { rerender } = render(
      <StateGate auth={makeAuth({ status: 'no_key' })}>
        <div>REAL AUTHENTICATED CONTENT</div>
      </StateGate>,
    );
    expect(screen.queryByText('REAL AUTHENTICATED CONTENT')).toBeNull();
    expect(screen.getByText('NO KEY CONFIGURED')).toBeTruthy();

    rerender(
      <StateGate auth={makeAuth({ status: 'unauthenticated' })}>
        <div>REAL AUTHENTICATED CONTENT</div>
      </StateGate>,
    );
    expect(screen.queryByText('REAL AUTHENTICATED CONTENT')).toBeNull();
    expect(screen.getByText(/AUTHENTICATION REQUIRED · 401/)).toBeTruthy();

    rerender(
      <StateGate auth={makeAuth({ status: 'key_present' })}>
        <div>REAL AUTHENTICATED CONTENT</div>
      </StateGate>,
    );
    expect(screen.getByText('REAL AUTHENTICATED CONTENT')).toBeTruthy();

    rerender(
      <StateGate auth={makeAuth({ status: 'checking' })}>
        <div>REAL AUTHENTICATED CONTENT</div>
      </StateGate>,
    );
    expect(screen.getByText('REAL AUTHENTICATED CONTENT')).toBeTruthy();

    rerender(
      <StateGate auth={makeAuth({ status: 'authenticated' })}>
        <div>REAL AUTHENTICATED CONTENT</div>
      </StateGate>,
    );
    expect(screen.getByText('REAL AUTHENTICATED CONTENT')).toBeTruthy();
  });
});

// -------------------------------------------------- driven by the real AuthContext

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}
function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_test' } };
}

/** Mounts the real `AuthProvider` and renders through the real
 *  `renderAuthState` on every re-render — nothing here is a static mock. */
const LiveConsumer: React.FC = () => {
  const auth = useAuth();
  return <>{renderAuthState(auth)}</>;
};

describe('Driven by the real FE-2 AuthContext state machine (critical rule)', () => {
  it('full live flow (covers items 1/2/4/5/16) — no_key -> save -> checking -> authenticated, then sign out -> no_key, all via the real context', async () => {
    let resolveFetch: (r: Response) => void;
    vi.stubGlobal(
      'fetch',
      vi.fn().mockReturnValue(
        new Promise<Response>((resolve) => {
          resolveFetch = resolve;
        }),
      ),
    );

    let authRef!: ReturnType<typeof useAuth>;
    const Capture: React.FC = () => {
      authRef = useAuth();
      return <LiveConsumer />;
    };

    render(
      <AuthProvider>
        <Capture />
      </AuthProvider>,
    );

    expect(screen.getByText(/NO KEY CONFIGURED/i)).toBeTruthy();

    let savePromise!: Promise<void>;
    act(() => {
      savePromise = authRef.saveKey('aiqa_livekey00000000000');
    });
    // The real state machine transitions through `checking` before the
    // network call resolves.
    expect(screen.getByText('VERIFYING…')).toBeTruthy();

    await act(async () => {
      resolveFetch!(jsonResponse(200, []));
      await savePromise;
    });

    expect(screen.getByText(/CONNECTED/i)).toBeTruthy();
    expect(screen.getByText(/aiqa_l.*•+/)).toBeTruthy();

    act(() => {
      authRef.signOut();
    });
    expect(screen.getByText(/NO KEY CONFIGURED/i)).toBeTruthy();
  });

  it('a real 401 from the backend drives the view to the 401 state, and a real 403 drives it to the distinct 403 state', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce(jsonResponse(401, envelope('UNAUTHENTICATED', 'invalid or revoked API key')))
      .mockResolvedValueOnce(jsonResponse(403, envelope('FORBIDDEN', 'resource belongs to a different project')));
    vi.stubGlobal('fetch', fetchMock);

    let authRef!: ReturnType<typeof useAuth>;
    const Capture: React.FC = () => {
      authRef = useAuth();
      return <LiveConsumer />;
    };
    render(
      <AuthProvider>
        <Capture />
      </AuthProvider>,
    );

    await act(async () => {
      await authRef.saveKey('aiqa_badkey00000000000000');
    });
    expect(screen.getByText(/AUTHENTICATION REQUIRED · 401/i)).toBeTruthy();

    await act(async () => {
      await authRef.validate();
    });
    expect(screen.getByText(/ACCESS FORBIDDEN · 403/i)).toBeTruthy();
    expect(screen.queryByText(/AUTHENTICATION REQUIRED · 401/i)).toBeNull();
  });

  it('a real network failure drives the view to the offline state, and the key stays configured', async () => {
    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new TypeError('Failed to fetch')));

    let authRef!: ReturnType<typeof useAuth>;
    const Capture: React.FC = () => {
      authRef = useAuth();
      return <LiveConsumer />;
    };
    render(
      <AuthProvider>
        <Capture />
      </AuthProvider>,
    );

    await act(async () => {
      await authRef.saveKey('aiqa_anykey0000000000000');
    });

    expect(screen.getByText('ORCHESTRATOR UNREACHABLE')).toBeTruthy();
    expect(authRef.hasKey).toBe(true);
  });
});

describe('renderResourceErrorState (FE-12 — centralized per-resource 401/403/offline/error mapping)', () => {
  it('401 and 403 render distinct labels, titles, and descriptions — never interchangeable', async () => {
    const { renderResourceErrorState } = await import('./StateViews');
    const onRetry = vi.fn();
    const copy = { forbiddenTitle: 'Not permitted to view this widget', forbiddenDescription: 'Role too low.' };

    const { unmount } = render(<>{renderResourceErrorState(new ApiError('unauthorized', 'x', 401), onRetry, copy)}</>);
    expect(screen.getByText('AUTHENTICATION REQUIRED · 401')).toBeTruthy();
    expect(screen.getByText('Authentication required')).toBeTruthy();
    expect(screen.queryByText('ACCESS FORBIDDEN · 403')).toBeNull();
    unmount();

    render(<>{renderResourceErrorState(new ApiError('forbidden', 'x', 403), onRetry, copy)}</>);
    expect(screen.getByText('ACCESS FORBIDDEN · 403')).toBeTruthy();
    expect(screen.getByText('Not permitted to view this widget')).toBeTruthy();
    expect(screen.getByText('Role too low.')).toBeTruthy();
    expect(screen.queryByText('AUTHENTICATION REQUIRED · 401')).toBeNull();
  });

  it('offline uses the page-specific suffix when given, and the plain sentence when omitted', async () => {
    const { renderResourceErrorState } = await import('./StateViews');
    const onRetry = vi.fn();

    const { unmount } = render(
      <>{renderResourceErrorState(new ApiError('offline', 'x'), onRetry, { forbiddenTitle: 't', forbiddenDescription: 'd', offlineDescriptionSuffix: 'to load widgets.' })}</>,
    );
    expect(screen.getByText('Unable to reach the AIQA Orchestrator to load widgets.')).toBeTruthy();
    unmount();

    render(<>{renderResourceErrorState(new ApiError('offline', 'x'), onRetry, { forbiddenTitle: 't', forbiddenDescription: 'd' })}</>);
    expect(screen.getByText('Unable to reach the AIQA Orchestrator.')).toBeTruthy();
  });

  it('the generic error branch uses the page-specific title when given, and a sensible default otherwise', async () => {
    const { renderResourceErrorState } = await import('./StateViews');
    const onRetry = vi.fn();

    const { unmount } = render(
      <>{renderResourceErrorState(new ApiError('server_error', 'db down'), onRetry, { forbiddenTitle: 't', forbiddenDescription: 'd', genericTitle: "Couldn't load widgets" })}</>,
    );
    expect(screen.getByText("Couldn't load widgets")).toBeTruthy();
    expect(screen.getByText('db down')).toBeTruthy();
    unmount();

    render(<>{renderResourceErrorState(new ApiError('server_error', 'db down'), onRetry, { forbiddenTitle: 't', forbiddenDescription: 'd' })}</>);
    expect(screen.getByText("Couldn't load this data")).toBeTruthy();
  });

  it('a real request_id renders only when the error actually carried one', async () => {
    const { renderResourceErrorState } = await import('./StateViews');
    const onRetry = vi.fn();
    const copy = { forbiddenTitle: 't', forbiddenDescription: 'd' };

    const { unmount } = render(<>{renderResourceErrorState(new ApiError('server_error', 'x', 500, null, null, 'req_abc123'), onRetry, copy)}</>);
    expect(screen.getByText(/request_id: req_abc123/)).toBeTruthy();
    unmount();

    render(<>{renderResourceErrorState(new ApiError('server_error', 'x'), onRetry, copy)}</>);
    expect(screen.queryByText(/request_id:/)).toBeNull();
  });
});
