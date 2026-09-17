/**
 * FE-5 — real, centralized project-scope context.
 *
 * Built strictly on FE-1's transport (`aiqaApi.projects.list`) and FE-2's
 * `AuthContext` — this is not a second authentication mechanism, it only
 * *reacts* to the one real `AuthStatus` FE-2 already computes. It never
 * calls `aiqaApi` directly except through the existing FE-1 abstraction.
 * (FE-11 removed `AIQAContext`'s own fabricated `AIQAProject[]` demo data
 * and its `projects`/`selectedProject` fields entirely — this context is
 * now the app's only "current project" concept. See this file's own
 * `Project` import: `types/contract.ts`'s real backend type.)
 *
 * Source-of-truth note (brief §1): no FRD/SRD/PRD/flow-diagram document
 * exists anywhere in this repository (re-confirmed at the start of this
 * sprint, same search as FE-4's own report §3). In that absence, every
 * project-selection/persistence/switching rule below is derived from the
 * real backend contract (`GET /api/v1/projects` returns only the caller's
 * own accessible projects — BE-8's `list_projects_handler` filters, it
 * never fabricates) and from this sprint's own brief where the contract is
 * silent. Each such judgment call is called out in its own comment and in
 * the FE-5 report §18, not silently assumed.
 */

import React, { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';
import { ApiError, aiqaApi } from '../services/aiqaApi';
import type { CreateProjectRequest, Project } from '../types/contract';
import { useAuth } from './AuthContext';

/** Deliberately a *different* storage key from `aiqaApi.ts`'s
 *  `aiqa.orchestrator.apiKey` (brief §8: "do not put project IDs into
 *  sensitive credential storage... do not create a second unrelated
 *  storage architecture" — this reuses the same `localStorage` mechanism,
 *  just its own key, and only ever stores an opaque id string, never a
 *  credential). Centralized here — no other file reads/writes it. */
const SELECTED_PROJECT_STORAGE = 'aiqa.selectedProjectId';

function readPersistedSelection(): string | null {
  try {
    return localStorage.getItem(SELECTED_PROJECT_STORAGE);
  } catch {
    return null;
  }
}

function persistSelection(id: string | null): void {
  try {
    if (id) localStorage.setItem(SELECTED_PROJECT_STORAGE, id);
    else localStorage.removeItem(SELECTED_PROJECT_STORAGE);
  } catch {
    /* storage unavailable (private mode) — selection just won't survive a refresh */
  }
}

export interface ProjectContextValue {
  /** The caller's real, backend-returned accessible projects. Empty until
   *  a successful fetch — never seeded with placeholder/demo entries. */
  projects: Project[];
  selectedProjectId: string | null;
  /** Derived from `projects`/`selectedProjectId` — `null` whenever the id
   *  doesn't (or no longer) resolves against the real list. */
  selectedProject: Project | null;
  /** True while a project-list request is in flight. */
  loading: boolean;
  /** The most recent project-list fetch failure, if any. Distinct from
   *  `AuthContext.lastError` — this is specifically about loading
   *  projects, not about the key itself (brief §17: don't confuse "no
   *  projects" with "network/auth failure"). */
  error: ApiError | null;
  /** Selects a project — but only if it is actually present in the real,
   *  last-fetched `projects` list. An id that doesn't resolve is silently
   *  ignored rather than "selected" anyway (brief §18: "project ID/name
   *  cannot be trusted as proof of permission" — this context never lets
   *  the UI assert access to something the backend didn't actually return). */
  selectProject: (projectId: string) => void;
  /** Re-fetches the real project list. No-ops outside `authenticated`
   *  (brief §5) — recovering from an auth-level failure (offline/401/...)
   *  goes through `AuthContext.validate()`, not this; see this file's own
   *  top-level comment on why that split is the correct one. */
  refreshProjects: () => Promise<void>;
  /** FE-14 — real `POST /api/v1/projects` via `aiqaApi.projects.create`.
   *  Resolves with the backend's own returned `Project` (never a locally
   *  fabricated one) and, on success, appends it to `projects` and selects
   *  it directly — the response body already *is* the authoritative record,
   *  so a second round-trip through `refreshProjects` would only re-fetch
   *  what this call already has. Rejects with the real `ApiError` on
   *  failure (e.g. `forbidden` for a non-admin key — project creation is
   *  Admin-only server-side, `api.rs::create_project`); callers must not
   *  treat a rejection as anything other than "nothing was created". */
  createProject: (body: CreateProjectRequest) => Promise<Project>;
  /** Real `DELETE /api/v1/projects/:id` via `aiqaApi.projects.delete`.
   *  Removes the project from the real list and, if the deleted project was
   *  the currently active project, safely switches to a fallback project or
   *  enters empty state without stale project state remaining. */
  deleteProject: (projectId: string) => Promise<void>;
}

export const ProjectContext = createContext<ProjectContextValue | undefined>(undefined);

export const ProjectProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const auth = useAuth();
  const [projects, setProjects] = useState<Project[]>([]);
  const [selectedProjectId, setSelectedProjectIdState] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  /** Reconciles a freshly-fetched real list against the current selection
   *  (Case D/E of brief §7: a stale or now-inaccessible selection is never
   *  silently kept) and, if nothing is currently selected, against the
   *  persisted id (Case: refresh persistence, brief §8) — but only ever
   *  adopting it after confirming real backend membership, never "blindly"
   *  (brief §16's own phrase, for the sign-in case). */
  const applyList = useCallback((list: Project[]) => {
    setProjects(list);
    setSelectedProjectIdState((current) => {
      if (current && list.some((p) => p.id === current)) return current;
      const persisted = readPersistedSelection();
      if (persisted && list.some((p) => p.id === persisted)) return persisted;
      return null;
    });
  }, []);

  const fetchProjects = useCallback(async () => {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;
    setLoading(true);
    setError(null);
    try {
      const list = await aiqaApi.projects.list(controller.signal);
      if (controller.signal.aborted) return;
      applyList(list);
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
      setError(apiErr);
      // A failed fetch means we cannot currently prove which (if any)
      // projects are accessible — never leave a stale/fabricated list
      // sitting next to an error state (brief §17).
      setProjects([]);
      setSelectedProjectIdState(null);
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  }, [applyList]);

  useEffect(() => {
    const status = auth.status;

    if (status === 'authenticated') {
      void fetchProjects();
      return;
    }

    // Purely transient states with no proven change to identity or
    // permission: `checking` is mid-flight (avoid a duplicate request —
    // brief §5), and `offline`/`error` mean the backend could not be
    // reached at all — the previously-loaded scope was never shown to be
    // wrong, so it is preserved rather than fabricated-fresh or nuked
    // (brief §5 "offline: preserve safe local scope state").
    if (status === 'checking' || status === 'offline' || status === 'error') {
      return;
    }

    // Every remaining status (`no_key`, `key_present`, `unauthenticated`,
    // `forbidden`) means project access cannot currently be proven for the
    // *current* credential — clear the in-memory scope so nothing stale
    // renders as if still authorized (brief §5, §16).
    abortRef.current?.abort();
    setLoading(false);
    setError(null);
    setProjects([]);
    setSelectedProjectIdState(null);

    // Only a proven-gone-or-rejected credential (no key at all, or a real
    // 401) wipes the *persisted* preference too — matching brief §16's
    // sign-out/401 requirement that nothing from the old session remains
    // reachable. `key_present` (a brand new, not-yet-verified credential)
    // and `forbidden` (the key itself is still valid) leave the persisted
    // id in storage: it can still be re-adopted by a *future* successful
    // authentication, but only through the same real-list membership check
    // `applyList` always performs — never reused blindly.
    if (status === 'no_key' || status === 'unauthenticated') {
      persistSelection(null);
    }
  }, [auth.status, fetchProjects]);

  // Cancel any in-flight request on unmount.
  useEffect(() => () => abortRef.current?.abort(), []);

  const selectProject = useCallback(
    (projectId: string) => {
      // The backend-returned list is the only source of truth for what
      // "accessible" means (brief §18) — an id this context has never seen
      // in a real response is never accepted as a selection.
      setProjects((currentList) => {
        if (currentList.some((p) => p.id === projectId)) {
          setSelectedProjectIdState(projectId);
          persistSelection(projectId);
        }
        return currentList;
      });
    },
    [],
  );

  const refreshProjects = useCallback(async () => {
    if (auth.status !== 'authenticated') return;
    await fetchProjects();
  }, [auth.status, fetchProjects]);

  const createProject = useCallback(async (body: CreateProjectRequest): Promise<Project> => {
    const project = await aiqaApi.projects.create(body);
    // The backend response is the real, authoritative record — append it
    // to the real list rather than re-fetching (brief §11: "avoid requiring
    // a browser reload"; a full refetch here would just re-download the
    // same object this response already carries).
    setProjects((current) => [...current, project]);
    setSelectedProjectIdState(project.id);
    persistSelection(project.id);
    return project;
  }, []);

  const deleteProject = useCallback(async (projectId: string): Promise<void> => {
    await aiqaApi.projects.delete(projectId);
    setProjects((current) => {
      const nextList = current.filter((p) => p.id !== projectId);
      setSelectedProjectIdState((currentSelected) => {
        if (currentSelected === projectId) {
          const fallback = nextList.length > 0 ? nextList[0].id : null;
          persistSelection(fallback);
          return fallback;
        }
        return currentSelected;
      });
      return nextList;
    });
  }, []);

  const selectedProject = useMemo(
    () => projects.find((p) => p.id === selectedProjectId) ?? null,
    [projects, selectedProjectId],
  );

  const value = useMemo<ProjectContextValue>(
    () => ({
      projects,
      selectedProjectId,
      selectedProject,
      loading,
      error,
      selectProject,
      refreshProjects,
      createProject,
      deleteProject,
    }),
    [projects, selectedProjectId, selectedProject, loading, error, selectProject, refreshProjects, createProject, deleteProject],
  );

  return <ProjectContext.Provider value={value}>{children}</ProjectContext.Provider>;
};

export const useProject = (): ProjectContextValue => {
  const ctx = useContext(ProjectContext);
  if (!ctx) {
    throw new Error('useProject must be used within a ProjectProvider');
  }
  return ctx;
};
