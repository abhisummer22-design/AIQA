/**
 * FE-5 targeted tests for the real project selector's UI consumption of
 * `ProjectContext` — items 17-20 of the brief's minimum test list (§20).
 * Renders the real `AppShell` (real providers, real `Header`/`Sidebar`,
 * real routed pages) inside a `MemoryRouter`, exactly like FE-4's own
 * `AppRoutes.test.tsx`. `fetch` is mocked only at the transport boundary.
 *
 * Note: this context deliberately never auto-selects a project, even when
 * there is exactly one (`ProjectContext.tsx`'s own documented judgment
 * call — brief §7 Case B explicitly warns against assuming auto-selection
 * without documentation support). So a freshly authenticated session shows
 * "Select a project", not the project's name, until a real selection is
 * made — these tests open the dropdown rather than assuming auto-select.
 */
import React from 'react';
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AppShell } from '../../App';
import { setApiKey } from '../../services/aiqaApi';
import type { Project } from '../../types/contract';

function jsonResponse(status: number, body: unknown): Response {
  if (status === 204 || body === null || body === undefined) {
    return new Response(null, { status, headers: { 'Content-Type': 'application/json' } });
  }
  return new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
}

const REAL_PROJECT: Project = {
  id: 'proj-real-selector',
  name: 'Real Selector Project',
  description: null,
  created_at: '2026-01-01T00:00:00Z',
  updated_at: '2026-01-01T00:00:00Z',
};
const RAW_KEY = 'aiqa_selectortest00000000';

function renderApp(initialEntries: string[] = ['/dashboard']) {
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

function selectorButton(): HTMLElement {
  return document.getElementById('project-selector-button')!;
}

/** Waits for the real project fetch (triggered by `verifyKey()`) to finish. */
async function waitForProjectsLoaded(): Promise<void> {
  await waitFor(() => expect(selectorButton().textContent).not.toContain('Loading projects'));
}

async function openSelector(): Promise<void> {
  await act(async () => {
    selectorButton().click();
  });
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});

afterEach(() => {
  localStorage.clear();
});

describe('ProjectSelector (FE-5 §20 items 17-20)', () => {
  it('item 17 — the raw API key never appears anywhere in the rendered selector output', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [REAL_PROJECT])));
    renderApp();
    await verifyKey();
    await waitForProjectsLoaded();
    await openSelector();

    // Scoped to the header's own real listbox — FE-10's Dashboard has its
    // own real `<select>` of the same projects, colliding on an unscoped
    // "option" role query.
    await waitFor(() => expect(within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: REAL_PROJECT.name })).toBeTruthy());

    expect(document.body.innerHTML).not.toContain(RAW_KEY);
  });

  it('item 18 — selecting a project never puts anything into the URL', async () => {
    setApiKey(RAW_KEY);
    // FE-10: this test genuinely selects a project (below), which now
    // triggers Dashboard's real `/dashboard`/`/kpis`/`/executions` fetches
    // — URL-aware so those get real-shaped responses, not the bare
    // `[REAL_PROJECT]` array every other call in this test still expects.
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.includes('/dashboard')) {
          return jsonResponse(200, {
            project_id: REAL_PROJECT.id,
            project_name: REAL_PROJECT.name,
            execution_counts: {},
            task_counts: {},
            pass_fail: { passed: 0, failed: 0 },
            test_count: 0,
            generated_at: '2026-08-19T00:00:00Z',
          });
        }
        if (url.includes('/kpis')) {
          return jsonResponse(200, { project_id: REAL_PROJECT.id, execution_reliability: null, execution_efficiency_ms: null, generated_at: '2026-08-19T00:00:00Z' });
        }
        if (url.includes('/executions')) return jsonResponse(200, []);
        return jsonResponse(200, [REAL_PROJECT]);
      }),
    );
    renderApp();
    await verifyKey();
    await waitForProjectsLoaded();
    await openSelector();
    await waitFor(() => expect(within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: REAL_PROJECT.name })).toBeTruthy());

    await act(async () => {
      within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: REAL_PROJECT.name }).click();
    });

    // The selection did take effect (button now shows the real name)...
    await waitFor(() => expect(selectorButton().textContent).toContain(REAL_PROJECT.name));
    // ...but context-only scope (this sprint's brief §15: no documented
    // URL/route requirement was found, so selection stays centralized in
    // context, never the URL) — and never a credential either way (§18).
    expect(window.location.href).not.toContain(RAW_KEY);
    expect(window.location.search).toBe('');
    expect(window.location.pathname).not.toContain(REAL_PROJECT.id);
  });

  it('item 19 — Dashboard still renders its existing product-owner-approved content, now with real data (FE-10)', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal(
      'fetch',
      vi.fn().mockImplementation(async (url: string) => {
        if (url.includes('/dashboard')) {
          return jsonResponse(200, {
            project_id: REAL_PROJECT.id,
            project_name: REAL_PROJECT.name,
            execution_counts: {},
            task_counts: {},
            pass_fail: { passed: 0, failed: 0 },
            test_count: 0,
            generated_at: '2026-08-19T00:00:00Z',
          });
        }
        if (url.includes('/kpis')) {
          return jsonResponse(200, { project_id: REAL_PROJECT.id, execution_reliability: null, execution_efficiency_ms: null, generated_at: '2026-08-19T00:00:00Z' });
        }
        if (url.includes('/executions')) return jsonResponse(200, []);
        return jsonResponse(200, [REAL_PROJECT]);
      }),
    );
    renderApp(['/dashboard']);
    await verifyKey();
    await waitForProjectsLoaded();
    await openSelector();
    // Scoped to the header's own real listbox — FE-10's Dashboard has its
    // own real `<select>` of the same projects, colliding on an unscoped
    // "option" role query.
    await act(async () => {
      within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: REAL_PROJECT.name }).click();
    });

    // FE-10: Dashboard now renders real project-scoped data — the same
    // real project the header's own real selector just chose, not a
    // fabricated seed project.
    await waitFor(() => expect(screen.getAllByText(REAL_PROJECT.name).length).toBeGreaterThan(0));
    expect(screen.getByText('Executed Tests')).toBeTruthy();
    expect(document.getElementById('aiqa-dashboard-main-content')).toBeTruthy();
    expect(selectorButton()).toBeTruthy();
  });

  it('item 20 — Test Generator still renders its existing product-owner-approved content unregressed', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [REAL_PROJECT])));
    renderApp(['/test-generator']);
    await verifyKey();

    await waitFor(() => expect(document.getElementById('aiqa-test-case-generator-page')).toBeTruthy());
    // "Test Case Generator" is legitimately ambiguous (Sidebar label + page
    // heading, pre-existing — see FE-4's own report §"item 18" fix).
    expect(screen.getAllByText('Test Case Generator').length).toBeGreaterThan(0);
    expect(screen.getByText('Ready to Generate')).toBeTruthy();
    await waitForProjectsLoaded();
    expect(selectorButton()).toBeTruthy();
  });

  it('empty accessible-projects state renders a real empty state, not a fabricated project', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [])));
    renderApp();
    await verifyKey();

    await waitFor(() => expect(selectorButton().textContent).toContain('No accessible projects'));

    await openSelector();
    expect(screen.getByText(/has no projects assigned/)).toBeTruthy();
  });

  it('single accessible project is not auto-selected (brief §7 Case B — no documentation supports it)', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [REAL_PROJECT])));
    renderApp();
    await verifyKey();

    await waitFor(() => expect(selectorButton().textContent).toContain('Select a project'));
  });
});

/**
 * FE-14 (GAP-01) — real Project Creation UI. Same real-`AppShell`,
 * transport-boundary-mocked-`fetch` approach as the FE-5 suite above. Every
 * scenario here maps to this sprint's brief §17/§19 minimum list.
 */
describe('ProjectSelector — FE-14 real project creation (GAP-01)', () => {
  const NEW_PROJECT: Project = {
    id: 'proj-fe14-created',
    name: 'Freshly Created Project',
    description: null,
    created_at: '2026-08-19T00:00:00Z',
    updated_at: '2026-08-19T00:00:00Z',
  };

  function errorEnvelope(code: string, message: string) {
    return { error: { code, message, details: {}, request_id: 'req_fe14test' } };
  }

  function nameInput(): HTMLElement {
    return screen.getByLabelText('Name');
  }

  function createButtonInList(): HTMLElement {
    return within(screen.getByRole('listbox', { name: 'Projects' })).getByText('Create Project').closest('button')!;
  }

  it('scenario A — the create entry point is reachable for an existing-project user', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [REAL_PROJECT])));
    renderApp();
    await verifyKey();
    await waitForProjectsLoaded();
    await openSelector();
    expect(createButtonInList()).toBeTruthy();
  });

  it('scenario A / §15 — the create entry point is reachable for a zero-project user, no pre-existing selection required', async () => {
    setApiKey(RAW_KEY);
    vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [])));
    renderApp();
    await verifyKey();
    await waitFor(() => expect(selectorButton().textContent).toContain('No accessible projects'));
    await openSelector();
    expect(createButtonInList()).toBeTruthy();
  });

  it('scenario B — an empty required name is rejected truthfully, with no request sent and no fake project created', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = vi.fn().mockImplementation(async () => jsonResponse(200, [REAL_PROJECT]));
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await verifyKey();
    await waitForProjectsLoaded();
    await openSelector();

    await act(async () => {
      createButtonInList().click();
    });
    const callsBeforeSubmit = fetchMock.mock.calls.length;

    await act(async () => {
      within(screen.getByRole('listbox', { name: 'New Project' })).getByText('Create Project').closest('button')!.click();
    });

    expect(screen.getByText('Project name is required.')).toBeTruthy();
    // No new POST /api/v1/projects request was made for the rejected submit.
    expect(fetchMock.mock.calls.length).toBe(callsBeforeSubmit);
  });

  it('scenario C — a valid submission calls the real API, the new project enters state, and becomes selected without a reload', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url === '/api/v1/projects' && init?.method === 'POST') {
        expect(JSON.parse(init.body as string)).toEqual({ name: 'Freshly Created Project' });
        return jsonResponse(201, NEW_PROJECT);
      }
      // Selecting the newly created project (below) triggers Dashboard's
      // own real `/dashboard`/`/kpis`/`/executions` fetches for it — same
      // URL-aware shape the existing FE-5/FE-10 tests above already use.
      if (url.includes('/dashboard')) {
        return jsonResponse(200, {
          project_id: NEW_PROJECT.id,
          project_name: NEW_PROJECT.name,
          execution_counts: {},
          task_counts: {},
          pass_fail: { passed: 0, failed: 0 },
          test_count: 0,
          generated_at: '2026-08-19T00:00:00Z',
        });
      }
      if (url.includes('/kpis')) {
        return jsonResponse(200, {
          project_id: NEW_PROJECT.id,
          execution_reliability: null,
          execution_efficiency_ms: null,
          generated_at: '2026-08-19T00:00:00Z',
        });
      }
      if (url.includes('/executions')) return jsonResponse(200, []);
      return jsonResponse(200, [REAL_PROJECT]);
    });
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await verifyKey();
    await waitForProjectsLoaded();
    await openSelector();

    await act(async () => {
      createButtonInList().click();
    });
    await act(async () => {
      (nameInput() as HTMLInputElement).focus();
    });
    fireEvent.change(nameInput(), { target: { value: 'Freshly Created Project' } });

    await act(async () => {
      within(screen.getByRole('listbox', { name: 'New Project' })).getByText('Create Project').closest('button')!.click();
    });

    // Real backend confirmed success -> the new project is now the active
    // selection, reflected purely through React state (no `location.reload`
    // call happened — jsdom would otherwise not be here to assert against).
    await waitFor(() => expect(selectorButton().textContent).toContain(NEW_PROJECT.name));

    await openSelector();
    await waitFor(() =>
      expect(
        within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: NEW_PROJECT.name }),
      ).toBeTruthy(),
    );
    // The pre-existing real project is still present too — creation appends,
    // it does not replace the real list.
    expect(within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: REAL_PROJECT.name })).toBeTruthy();
  });

  it('scenario D — a real backend failure (403, admin-only) shows a truthful error and creates no fake project', async () => {
    setApiKey(RAW_KEY);
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url === '/api/v1/projects' && init?.method === 'POST') {
        return jsonResponse(403, errorEnvelope('FORBIDDEN', "role 'operator' cannot perform this action"));
      }
      return jsonResponse(200, [REAL_PROJECT]);
    });
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await verifyKey();
    await waitForProjectsLoaded();
    await openSelector();

    await act(async () => {
      createButtonInList().click();
    });
    fireEvent.change(nameInput(), { target: { value: 'Should Not Be Created' } });

    await act(async () => {
      within(screen.getByRole('listbox', { name: 'New Project' })).getByText('Create Project').closest('button')!.click();
    });

    await waitFor(() => expect(screen.getByText("role 'operator' cannot perform this action")).toBeTruthy());
    // Trigger button still shows the pre-existing real selection state, not
    // a fabricated success — and the rejected project never entered the list.
    expect(selectorButton().textContent).not.toContain('Should Not Be Created');
  });

  it('scenario E — duplicate submission is prevented while a creation request is pending', async () => {
    setApiKey(RAW_KEY);
    let resolveCreate!: (value: Response) => void;
    const createPromise = new Promise<Response>((resolve) => {
      resolveCreate = resolve;
    });
    let postCallCount = 0;
    const fetchMock = vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
      if (url === '/api/v1/projects' && init?.method === 'POST') {
        postCallCount += 1;
        return createPromise;
      }
      // Same reason as scenario C — selecting the newly created project
      // triggers Dashboard's own real project-scoped fetches.
      if (url.includes('/dashboard')) {
        return jsonResponse(200, {
          project_id: 'proj-fe14-pending',
          project_name: 'Pending Project',
          execution_counts: {},
          task_counts: {},
          pass_fail: { passed: 0, failed: 0 },
          test_count: 0,
          generated_at: '2026-08-19T00:00:00Z',
        });
      }
      if (url.includes('/kpis')) {
        return jsonResponse(200, {
          project_id: 'proj-fe14-pending',
          execution_reliability: null,
          execution_efficiency_ms: null,
          generated_at: '2026-08-19T00:00:00Z',
        });
      }
      if (url.includes('/executions')) return jsonResponse(200, []);
      return jsonResponse(200, [REAL_PROJECT]);
    });
    vi.stubGlobal('fetch', fetchMock);
    renderApp();
    await verifyKey();
    await waitForProjectsLoaded();
    await openSelector();

    await act(async () => {
      createButtonInList().click();
    });
    fireEvent.change(nameInput(), { target: { value: 'Pending Project' } });

    const submitButton = () => document.getElementById('create-project-submit-button') as HTMLButtonElement;

    await act(async () => {
      submitButton().click();
    });
    // A second click while the first request is still in flight — the
    // button is real-disabled during `createSubmitting`, so this must not
    // fire a second POST.
    await act(async () => {
      submitButton().click();
    });

    expect(postCallCount).toBe(1);

    await act(async () => {
      resolveCreate(jsonResponse(201, { ...NEW_PROJECT, name: 'Pending Project' }));
      await createPromise;
    });
    await waitFor(() => expect(selectorButton().textContent).toContain('Pending Project'));
    expect(postCallCount).toBe(1);
  });

  describe('Delete Project UI integration', () => {
    const PROJECT_ALPHA: Project = {
      id: 'proj-alpha',
      name: 'Alpha Project',
      description: null,
      created_at: '2026-01-01T00:00:00Z',
      updated_at: '2026-01-01T00:00:00Z',
    };
    const PROJECT_BETA: Project = {
      id: 'proj-beta',
      name: 'Beta Project',
      description: null,
      created_at: '2026-01-02T00:00:00Z',
      updated_at: '2026-01-02T00:00:00Z',
    };

    it('each project has a delete button that opens the confirmation dialog without selecting the project', async () => {
      setApiKey(RAW_KEY);
      vi.stubGlobal('fetch', vi.fn().mockImplementation(async () => jsonResponse(200, [PROJECT_ALPHA, PROJECT_BETA])));
      renderApp();
      await verifyKey();
      await waitForProjectsLoaded();
      await openSelector();

      const deleteAlphaBtn = screen.getByLabelText('Delete project Alpha Project');
      expect(deleteAlphaBtn).toBeTruthy();

      await act(async () => {
        deleteAlphaBtn.click();
      });

      // Dialog opens
      expect(document.getElementById('delete-project-modal')).toBeTruthy();
      expect(screen.getByText('Delete Project?')).toBeTruthy();
      expect(screen.getByText(/You are about to permanently delete/)).toBeTruthy();
      expect(screen.getByText(/"Alpha Project"/)).toBeTruthy();

      // Cancel closes dialog
      const cancelBtn = screen.getByRole('button', { name: 'Cancel' });
      await act(async () => {
        cancelBtn.click();
      });
      expect(document.getElementById('delete-project-modal')).toBeNull();
    });

    it('deleting active project shows the active project warning banner', async () => {
      setApiKey(RAW_KEY);
      vi.stubGlobal(
        'fetch',
        vi.fn().mockImplementation(async (url: string) => {
          if (url.includes('/dashboard')) return jsonResponse(200, { project_id: 'proj-alpha', project_name: 'Alpha Project', execution_counts: {}, task_counts: {}, pass_fail: { passed: 0, failed: 0 }, test_count: 0, generated_at: '2026-08-19T00:00:00Z' });
          if (url.includes('/kpis')) return jsonResponse(200, { project_id: 'proj-alpha', execution_reliability: null, execution_efficiency_ms: null, generated_at: '2026-08-19T00:00:00Z' });
          if (url.includes('/executions')) return jsonResponse(200, []);
          return jsonResponse(200, [PROJECT_ALPHA, PROJECT_BETA]);
        }),
      );
      renderApp();
      await verifyKey();
      await waitForProjectsLoaded();
      await openSelector();

      // Select Alpha Project
      await act(async () => {
        within(screen.getByRole('listbox', { name: 'Projects' })).getByRole('option', { name: PROJECT_ALPHA.name }).click();
      });
      await waitFor(() => expect(selectorButton().textContent).toContain('Alpha Project'));

      // Open selector and click delete on Alpha
      await openSelector();
      const deleteAlphaBtn = screen.getByLabelText('Delete project Alpha Project');
      await act(async () => {
        deleteAlphaBtn.click();
      });

      expect(screen.getByText('Delete Active Project?')).toBeTruthy();
      expect(screen.getByText(/This project is currently/)).toBeTruthy();
    });

    it('confirming delete calls DELETE endpoint, deletes project, and shows success message', async () => {
      setApiKey(RAW_KEY);
      let deleteCalled = false;
      vi.stubGlobal(
        'fetch',
        vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
          if (init?.method === 'DELETE') {
            deleteCalled = true;
            return jsonResponse(204, null);
          }
          if (url.includes('/dashboard')) return jsonResponse(200, { project_id: 'proj-alpha', project_name: 'Alpha Project', execution_counts: {}, task_counts: {}, pass_fail: { passed: 0, failed: 0 }, test_count: 0, generated_at: '2026-08-19T00:00:00Z' });
          if (url.includes('/kpis')) return jsonResponse(200, { project_id: 'proj-alpha', execution_reliability: null, execution_efficiency_ms: null, generated_at: '2026-08-19T00:00:00Z' });
          if (url.includes('/executions')) return jsonResponse(200, []);
          return jsonResponse(200, [PROJECT_ALPHA, PROJECT_BETA]);
        }),
      );
      renderApp();
      await verifyKey();
      await waitForProjectsLoaded();
      await openSelector();

      const deleteBetaBtn = screen.getByLabelText('Delete project Beta Project');
      await act(async () => {
        deleteBetaBtn.click();
      });

      const confirmBtn = document.getElementById('confirm-delete-project-button')!;
      await act(async () => {
        confirmBtn.click();
      });

      expect(deleteCalled).toBe(true);
      expect(document.getElementById('delete-project-modal')).toBeNull();
      await waitFor(() => expect(screen.getByText('Project "Beta Project" deleted successfully.')).toBeTruthy());
    });

    it('delete failure renders error in modal and keeps modal open', async () => {
      setApiKey(RAW_KEY);
      vi.stubGlobal(
        'fetch',
        vi.fn().mockImplementation(async (url: string, init?: RequestInit) => {
          if (init?.method === 'DELETE') {
            return jsonResponse(500, { error: { code: 'INTERNAL', message: 'Database delete failure', details: {}, request_id: 'req_1' } });
          }
          return jsonResponse(200, [PROJECT_ALPHA]);
        }),
      );
      renderApp();
      await verifyKey();
      await waitForProjectsLoaded();
      await openSelector();

      const deleteBtn = screen.getByLabelText('Delete project Alpha Project');
      await act(async () => {
        deleteBtn.click();
      });

      const confirmBtn = document.getElementById('confirm-delete-project-button')!;
      await act(async () => {
        confirmBtn.click();
      });

      expect(document.getElementById('delete-project-modal')).toBeTruthy();
      expect(screen.getByText('Database delete failure')).toBeTruthy();
    });
  });
});
