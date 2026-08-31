/**
 * FE-5 targeted tests for the real project-scope context — the brief's
 * minimum test list (§20), exercised through the real `AuthProvider` +
 * `ProjectProvider` + `useAuth`/`useProject`, with `fetch` mocked only at
 * the transport boundary (same pattern as FE-2's `AuthContext.test.tsx`).
 */
import React from 'react';
import { act, renderHook, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AuthProvider, useAuth } from './AuthContext';
import { ProjectProvider, useProject } from './ProjectContext';
import { setApiKey } from '../services/aiqaApi';
import type { Project } from '../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  if (status === 204 || body === null || body === undefined) {
    return new Response(null, { status, headers: { 'Content-Type': 'application/json' } });
  }
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}
function envelope(code: string, message: string) {
  return { error: { code, message, details: {}, request_id: 'req_test' } };
}

const PROJECT_A: Project = {
  id: 'proj-real-a',
  name: 'Real Project Alpha',
  description: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};
const PROJECT_B: Project = {
  id: 'proj-real-b',
  name: 'Real Project Beta',
  description: null,
  created_at: '2026-01-02T00:00:00Z',
  updated_at: '2026-01-02T00:00:00Z',
};

const wrapper = ({ children }: { children: React.ReactNode }) => (
  <AuthProvider>
    <ProjectProvider>{children}</ProjectProvider>
  </AuthProvider>
);

function useCombined() {
  return { auth: useAuth(), project: useProject() };
}

/** Drives the real auth state machine to `authenticated` via the real
 *  "Verify Connection" call path (`validate()`), exactly like FE-4's own
 *  `verifyKey()` helper — never a test-only backdoor. */
async function authenticate(result: { current: ReturnType<typeof useCombined> }, projectsResponse: Project[]) {
  setApiKey('aiqa_projecttest0000000');
  vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, projectsResponse)));
  await act(async () => {
    await result.current.auth.validate();
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
  vi.unstubAllGlobals();
});

describe('ProjectContext (FE-5 §20)', () => {
  it('item 1/2 — no_key and unauthenticated never fetch projects', async () => {
    // no_key: nothing stored at all.
    const { result: r1 } = renderHook(() => useCombined(), { wrapper });
    expect(r1.current.auth.status).toBe('no_key');
    expect(r1.current.project.projects).toEqual([]);
    expect(r1.current.project.loading).toBe(false);

    // unauthenticated: a real 401 from the validate() call itself.
    localStorage.clear();
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(401, envelope('UNAUTHENTICATED', 'bad key')));
    vi.stubGlobal('fetch', fetchMock);
    setApiKey('aiqa_badkey00000000000');
    const { result: r2 } = renderHook(() => useCombined(), { wrapper });
    await act(async () => {
      await r2.current.auth.validate();
    });
    expect(r2.current.auth.status).toBe('unauthenticated');
    // Exactly one fetch call: AuthContext's own probe. ProjectContext must
    // not have issued a second, separate request for this status.
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(r2.current.project.projects).toEqual([]);
  });

  it('item 3 — key_present (unverified) does not falsely act as authenticated and never fetches', () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    setApiKey('aiqa_neververified00000');

    const { result } = renderHook(() => useCombined(), { wrapper });

    expect(result.current.auth.status).toBe('key_present');
    expect(result.current.project.projects).toEqual([]);
    expect(result.current.project.selectedProject).toBeNull();
    expect(result.current.project.loading).toBe(false);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('item 4/5 — authenticated fetches the real project list and stores the real API response', async () => {
    const { result } = renderHook(() => useCombined(), { wrapper });
    await authenticate(result, [PROJECT_A, PROJECT_B]);

    expect(result.current.auth.status).toBe('authenticated');
    await waitFor(() => expect(result.current.project.projects).toEqual([PROJECT_A, PROJECT_B]));
    expect(result.current.project.loading).toBe(false);
    expect(result.current.project.error).toBeNull();
  });

  it('item 6/7/8 — a project can be selected, is available globally, and switching updates it', async () => {
    const { result } = renderHook(() => useCombined(), { wrapper });
    await authenticate(result, [PROJECT_A, PROJECT_B]);
    await waitFor(() => expect(result.current.project.projects.length).toBe(2));

    act(() => result.current.project.selectProject(PROJECT_A.id));
    expect(result.current.project.selectedProjectId).toBe(PROJECT_A.id);
    expect(result.current.project.selectedProject).toEqual(PROJECT_A);

    act(() => result.current.project.selectProject(PROJECT_B.id));
    expect(result.current.project.selectedProjectId).toBe(PROJECT_B.id);
    expect(result.current.project.selectedProject).toEqual(PROJECT_B);
  });

  it('item 9 — zero accessible projects is a real, distinct empty state (not an error)', async () => {
    const { result } = renderHook(() => useCombined(), { wrapper });
    await authenticate(result, []);

    await waitFor(() => expect(result.current.project.loading).toBe(false));
    expect(result.current.project.projects).toEqual([]);
    expect(result.current.project.selectedProject).toBeNull();
    expect(result.current.project.error).toBeNull();
  });

  it('item 10/12 — an unauthorized (401) project-list failure is a distinct, real error state', async () => {
    const { result } = renderHook(() => useCombined(), { wrapper });
    await authenticate(result, [PROJECT_A]);
    await waitFor(() => expect(result.current.project.projects.length).toBe(1));

    // Re-verify, but this time the same endpoint rejects the key.
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(401, envelope('UNAUTHENTICATED', 'revoked'))));
    await act(async () => {
      await result.current.auth.validate();
    });

    expect(result.current.auth.status).toBe('unauthenticated');
    expect(result.current.project.projects).toEqual([]);
    expect(result.current.project.selectedProject).toBeNull();
  });

  it('item 13 — a forbidden (403) result stays distinct from unauthorized and does not wipe persisted selection', async () => {
    const { result } = renderHook(() => useCombined(), { wrapper });
    await authenticate(result, [PROJECT_A]);
    await waitFor(() => expect(result.current.project.projects.length).toBe(1));
    act(() => result.current.project.selectProject(PROJECT_A.id));
    expect(localStorage.getItem('aiqa.selectedProjectId')).toBe(PROJECT_A.id);

    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(403, envelope('FORBIDDEN', 'not permitted'))));
    await act(async () => {
      await result.current.auth.validate();
    });

    expect(result.current.auth.status).toBe('forbidden');
    // In-memory scope is cleared (nothing stale renders as authorized)...
    expect(result.current.project.projects).toEqual([]);
    expect(result.current.project.selectedProject).toBeNull();
    // ...but unlike a proven-bad 401, the persisted preference is not
    // wiped — the key itself is still valid (FE-3: "the key stays
    // configured"), so a future successful re-verification may still
    // re-adopt it, subject to the same real-list membership check.
    expect(localStorage.getItem('aiqa.selectedProjectId')).toBe(PROJECT_A.id);
  });

  it('item 11 — an offline failure preserves the last-known-good scope instead of fabricating an empty list', async () => {
    const { result } = renderHook(() => useCombined(), { wrapper });
    await authenticate(result, [PROJECT_A, PROJECT_B]);
    await waitFor(() => expect(result.current.project.projects.length).toBe(2));
    act(() => result.current.project.selectProject(PROJECT_A.id));

    // Network-level failure (not an HTTP response at all) classifies as 'offline'.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockRejectedValue(new TypeError('Failed to fetch')),
    );
    await act(async () => {
      await result.current.auth.validate();
    });

    expect(result.current.auth.status).toBe('offline');
    // Preserved, not reset to [] / null and not silently re-fetched.
    expect(result.current.project.projects).toEqual([PROJECT_A, PROJECT_B]);
    expect(result.current.project.selectedProjectId).toBe(PROJECT_A.id);
  });

  it('item 14 — an id that is not in the real fetched list can never be selected', async () => {
    const { result } = renderHook(() => useCombined(), { wrapper });
    await authenticate(result, [PROJECT_A]);
    await waitFor(() => expect(result.current.project.projects.length).toBe(1));

    act(() => result.current.project.selectProject('does-not-exist-and-was-never-returned'));

    expect(result.current.project.selectedProjectId).toBeNull();
    expect(result.current.project.selectedProject).toBeNull();
  });

  it('item 15 — a stale persisted selection (project no longer returned) is cleared, not silently kept', async () => {
    localStorage.setItem('aiqa.selectedProjectId', 'a-project-that-no-longer-exists');
    const { result } = renderHook(() => useCombined(), { wrapper });
    await authenticate(result, [PROJECT_A]);

    await waitFor(() => expect(result.current.project.projects.length).toBe(1));
    expect(result.current.project.selectedProjectId).toBeNull();
    expect(result.current.project.selectedProject).toBeNull();
  });

  it('item 16 — signing out clears project-scoped state (memory and the persisted selection)', async () => {
    const { result } = renderHook(() => useCombined(), { wrapper });
    await authenticate(result, [PROJECT_A]);
    await waitFor(() => expect(result.current.project.projects.length).toBe(1));
    act(() => result.current.project.selectProject(PROJECT_A.id));
    expect(localStorage.getItem('aiqa.selectedProjectId')).toBe(PROJECT_A.id);

    act(() => result.current.auth.signOut());

    expect(result.current.auth.status).toBe('no_key');
    expect(result.current.project.projects).toEqual([]);
    expect(result.current.project.selectedProjectId).toBeNull();
    expect(localStorage.getItem('aiqa.selectedProjectId')).toBeNull();
  });

  it('item 21 — no fabricated AIQAContext project ever appears in the real project list', async () => {
    const { result } = renderHook(() => useCombined(), { wrapper });
    await authenticate(result, [PROJECT_A, PROJECT_B]);
    await waitFor(() => expect(result.current.project.projects.length).toBe(2));

    const ids = result.current.project.projects.map((p) => p.id);
    const names = result.current.project.projects.map((p) => p.name);
    // 'prj-aiqa' / 'AIQA Platform' is AIQAContext's own fabricated demo
    // project (see `AIQAContext.tsx`'s `projects` seed) — it must never
    // leak into the real, backend-driven list.
    expect(ids).not.toContain('prj-aiqa');
    expect(names).not.toContain('AIQA Platform');
  });

  it('refreshProjects is a no-op outside authenticated (does not fetch)', async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal('fetch', fetchMock);
    const { result } = renderHook(() => useCombined(), { wrapper });
    expect(result.current.auth.status).toBe('no_key');

    await act(async () => {
      await result.current.project.refreshProjects();
    });

    expect(fetchMock).not.toHaveBeenCalled();
  });

  it('refresh persistence: a valid persisted selection is re-adopted after re-authentication', async () => {
    const { result } = renderHook(() => useCombined(), { wrapper });
    await authenticate(result, [PROJECT_A, PROJECT_B]);
    await waitFor(() => expect(result.current.project.projects.length).toBe(2));
    act(() => result.current.project.selectProject(PROJECT_B.id));
    expect(localStorage.getItem('aiqa.selectedProjectId')).toBe(PROJECT_B.id);

    // Simulate a fresh mount (e.g. a page refresh) with the same persisted
    // storage and the same still-valid key.
    const { result: fresh } = renderHook(() => useCombined(), { wrapper });
    expect(fresh.current.auth.status).toBe('key_present');
    expect(fresh.current.project.selectedProjectId).toBeNull(); // not yet re-verified this session

    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [PROJECT_A, PROJECT_B])));
    await act(async () => {
      await fresh.current.auth.validate();
    });

    await waitFor(() => expect(fresh.current.project.selectedProjectId).toBe(PROJECT_B.id));
  });

  describe('deleteProject (real project deletion)', () => {
    it('deleting a non-active project removes it from list and keeps active project unchanged', async () => {
      const { result } = renderHook(() => useCombined(), { wrapper });
      await authenticate(result, [PROJECT_A, PROJECT_B]);
      await waitFor(() => expect(result.current.project.projects.length).toBe(2));

      act(() => result.current.project.selectProject(PROJECT_A.id));
      expect(result.current.project.selectedProjectId).toBe(PROJECT_A.id);

      const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (init?.method === 'DELETE') return jsonResponse(204, null);
        return jsonResponse(200, [PROJECT_A, PROJECT_B]);
      });
      vi.stubGlobal('fetch', fetchMock);

      await act(async () => {
        await result.current.project.deleteProject(PROJECT_B.id);
      });

      expect(result.current.project.projects).toEqual([PROJECT_A]);
      expect(result.current.project.selectedProjectId).toBe(PROJECT_A.id);
      expect(result.current.project.selectedProject).toEqual(PROJECT_A);
      expect(localStorage.getItem('aiqa.selectedProjectId')).toBe(PROJECT_A.id);
    });

    it('deleting the active project switches to fallback project and updates persistence', async () => {
      const { result } = renderHook(() => useCombined(), { wrapper });
      await authenticate(result, [PROJECT_A, PROJECT_B]);
      await waitFor(() => expect(result.current.project.projects.length).toBe(2));

      act(() => result.current.project.selectProject(PROJECT_A.id));
      expect(result.current.project.selectedProjectId).toBe(PROJECT_A.id);

      const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (init?.method === 'DELETE') return jsonResponse(204, null);
        return jsonResponse(200, [PROJECT_A, PROJECT_B]);
      });
      vi.stubGlobal('fetch', fetchMock);

      await act(async () => {
        await result.current.project.deleteProject(PROJECT_A.id);
      });

      expect(result.current.project.projects).toEqual([PROJECT_B]);
      expect(result.current.project.selectedProjectId).toBe(PROJECT_B.id);
      expect(result.current.project.selectedProject).toEqual(PROJECT_B);
      expect(localStorage.getItem('aiqa.selectedProjectId')).toBe(PROJECT_B.id);
    });

    it('deleting the only remaining project clears active selection safely', async () => {
      const { result } = renderHook(() => useCombined(), { wrapper });
      await authenticate(result, [PROJECT_A]);
      await waitFor(() => expect(result.current.project.projects.length).toBe(1));

      act(() => result.current.project.selectProject(PROJECT_A.id));
      expect(result.current.project.selectedProjectId).toBe(PROJECT_A.id);

      const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (init?.method === 'DELETE') return jsonResponse(204, null);
        return jsonResponse(200, [PROJECT_A]);
      });
      vi.stubGlobal('fetch', fetchMock);

      await act(async () => {
        await result.current.project.deleteProject(PROJECT_A.id);
      });

      expect(result.current.project.projects).toEqual([]);
      expect(result.current.project.selectedProjectId).toBeNull();
      expect(result.current.project.selectedProject).toBeNull();
      expect(localStorage.getItem('aiqa.selectedProjectId')).toBeNull();
    });

    it('a failed delete keeps state intact and throws ApiError', async () => {
      const { result } = renderHook(() => useCombined(), { wrapper });
      await authenticate(result, [PROJECT_A, PROJECT_B]);
      await waitFor(() => expect(result.current.project.projects.length).toBe(2));

      act(() => result.current.project.selectProject(PROJECT_A.id));

      const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
        if (init?.method === 'DELETE') return jsonResponse(500, envelope('INTERNAL', 'database error'));
        return jsonResponse(200, [PROJECT_A, PROJECT_B]);
      });
      vi.stubGlobal('fetch', fetchMock);

      let threw = false;
      await act(async () => {
        try {
          await result.current.project.deleteProject(PROJECT_A.id);
        } catch {
          threw = true;
        }
      });

      expect(threw).toBe(true);
      expect(result.current.project.projects).toEqual([PROJECT_A, PROJECT_B]);
      expect(result.current.project.selectedProjectId).toBe(PROJECT_A.id);
    });
  });
});
