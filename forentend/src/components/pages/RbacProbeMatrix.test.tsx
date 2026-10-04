/**
 * What these tests protect.
 *
 * `RbacProbeMatrix` renders a grid whose whole credibility rests on one thing:
 * every cell is a real answer the server gave, never a value the client decided.
 * The failure this page used to have was precisely the opposite -- a grid filled
 * in from a hardcoded table (FE-11) and later from source parsing (abandoned after
 * two parsers disagreed: 66 versus 8 routes without a role gate, and an
 * `ai-usage` row claiming admin when the route answers a viewer).
 *
 * So the invariants here are all about honesty, and they are the ones that would
 * be easy to reintroduce by "simplifying":
 *
 *  - no cell is pre-filled before a probe has run;
 *  - the cell's wording comes from the server's own `gate_label`, so the client
 *    never has its own vocabulary for what a refusal means;
 *  - "allowed" is not rendered as success: a 400 past the gate is shown as
 *    allowed-then-refused, because the two are different claims;
 *  - an unsound measurement (rejected key, unserved verb) is shown as such rather
 *    than as a permission result;
 *  - nothing is persisted: an API key must not reach localStorage from here;
 *  - a request this component aborted is not reported as a load failure, which is
 *    the one bug in this file's history that a unit test missed and driving the
 *    real page caught.
 */

import React from 'react';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RbacProbeMatrix } from './RbacProbeMatrix';
import { aiqaApi, ApiError } from '../../services/aiqaApi';
import type {
  RbacProbeResult,
  RbacProbeTargetsResponse,
} from '../../types/contract';

const TARGETS: RbacProbeTargetsResponse = {
  roles: ['viewer', 'operator', 'admin'],
  note: 'There is no required-role column, on purpose.',
  targets: [
    {
      id: 'metrics',
      method: 'GET',
      path: '/metrics',
      capability: 'Prometheus metrics (deployment-wide counters)',
      category: 'Deployment',
      mutating: false,
      creates: false,
    },
    {
      id: 'keys-create',
      method: 'POST',
      path: '/admin/api-keys',
      capability: 'Mint a new API key',
      category: 'Keys',
      mutating: true,
      creates: true,
    },
    {
      // Mutating but *not* creating: it names a `:project_id`, so a probe that got
      // past the gate would address a project that cannot exist. This is the case
      // the "non-existent uuid" wording is actually true for, and it is why the two
      // cases are labelled differently rather than lumped together.
      id: 'crawl-start',
      method: 'POST',
      path: '/api/v1/projects/:project_id/crawl',
      capability: 'Crawl a running application',
      category: 'Live application',
      mutating: true,
      creates: false,
    },
  ],
};

function result(over: Partial<RbacProbeResult>): RbacProbeResult {
  return {
    target_id: null,
    method: 'GET',
    path: '/metrics',
    status: 403,
    gate: 'role_refused',
    gate_label: 'blocked by the role gate',
    allowed: false,
    inconclusive: false,
    message: '',
    mutating: false,
    ...over,
  };
}

/** The row for a target, anchored on its capability sentence -- a single text node,
 *  unlike the route cell, whose `{method} {path}` is split and cannot be matched
 *  exactly. */
function rowFor(capability: string): HTMLElement {
  return screen.getByText(capability).closest('tr') as HTMLElement;
}

describe('RbacProbeMatrix', () => {
  beforeEach(() => {
    vi.spyOn(aiqaApi.rbac, 'probeTargets').mockResolvedValue(TARGETS);
  });

  afterEach(() => {
    vi.restoreAllMocks();
    window.localStorage.clear();
  });

  it('leaves every cell unprobed until a probe has actually run', async () => {
    render(<RbacProbeMatrix />);
    await screen.findByText('Prometheus metrics (deployment-wide counters)');

    // An unprobed cell must not read as either allowed or refused. "not probed" is
    // the only honest thing to show, because nobody has asked the server yet.
    expect(screen.getAllByText('not probed').length).toBe(3 * 3); // three targets x three roles
    expect(screen.queryByText(/allowed/i)).toBeNull();
    expect(screen.queryByText(/blocked/i)).toBeNull();
  });

  it("shows the server's own wording for a refusal, not the client's", async () => {
    vi.spyOn(aiqaApi.rbac, 'probe').mockResolvedValue(
      result({
        gate: 'scope_refused',
        gate_label: 'blocked by the project-scope gate',
        message: 'key is scoped to project abc and cannot act on def',
      }),
    );

    render(<RbacProbeMatrix />);
    await screen.findByText('Prometheus metrics (deployment-wide counters)');
    fireEvent.change(screen.getByLabelText(/keys to probe with/i), {
      target: { value: 'viewer=aiqa_viewer\noperator=aiqa_operator\nadmin=aiqa_admin' },
    });

    const row = rowFor('Prometheus metrics (deployment-wide counters)');
    fireEvent.click(within(row).getByRole('button', { name: /probe/i }));

    await waitFor(() =>
      // The label the server sent, verbatim. If the client had its own vocabulary
      // this would read differently, which is what the assertion is for.
      //
      // Three, not one: probing a row sends a request per role that has a key, and
      // three were pasted. One cell per role is the whole shape of the thing.
      expect(within(row).getAllByText('blocked by the project-scope gate').length).toBe(3),
    );
  });

  it('does not present "allowed by the gate, then refused" as success', async () => {
    vi.spyOn(aiqaApi.rbac, 'probe').mockResolvedValue(
      result({
        status: 422,
        gate: 'allowed_then_refused',
        gate_label: 'allowed by the gate, refused afterwards',
        allowed: true,
        message: 'missing field `title`',
      }),
    );

    render(<RbacProbeMatrix />);
    await screen.findByText('Prometheus metrics (deployment-wide counters)');
    fireEvent.change(screen.getByLabelText(/keys to probe with/i), {
      target: { value: 'viewer=aiqa_viewer' },
    });
    const row = rowFor('Prometheus metrics (deployment-wide counters)');
    fireEvent.click(within(row).getByRole('button', { name: /probe/i }));

    await waitFor(() =>
      expect(within(row).getByText('allowed by the gate, refused afterwards')).toBeTruthy(),
    );
    // The status is shown as 422, so a reader can see it was not a 2xx.
    expect(within(row).getByText('422')).toBeTruthy();
    // And the validation message that caused it is available, not swallowed.
    expect(screen.getByText(/missing field/)).toBeTruthy();
  });

  it('tells the two kinds of side effect apart on the row itself', async () => {
    render(<RbacProbeMatrix />);
    await screen.findByText('Mint a new API key');
    // Mutating without an id: the ghost-id reassurance genuinely applies.
    expect(screen.getByText(/every id is a non-existent uuid/i)).toBeTruthy();
    // Mutating with no id: it creates a row, so the reassurance must not appear.
    expect(screen.getByText(/creates a real record if it gets past the gate/i)).toBeTruthy();
    // A read-only target carries neither warning.
    const readOnly = screen.getByText('Prometheus metrics (deployment-wide counters)')
      .closest('tr') as HTMLElement;
    // `textContent`, not `innerText`: jsdom does not implement innerText.
    expect(readOnly.textContent ?? '').not.toMatch(
      /can change something|creates a real record/,
    );
  });

  it('probes with the key for that role, not the key the page is signed in with', async () => {
    const probe = vi.spyOn(aiqaApi.rbac, 'probe').mockResolvedValue(result({}));
    render(<RbacProbeMatrix />);
    await screen.findByText('Prometheus metrics (deployment-wide counters)');
    fireEvent.change(screen.getByLabelText(/keys to probe with/i), {
      target: { value: 'viewer=viewer_secret_key' },
    });
    const row = rowFor('Prometheus metrics (deployment-wide counters)');
    fireEvent.click(within(row).getByRole('button', { name: /probe/i }));

    await waitFor(() => expect(probe).toHaveBeenCalled());
    const body = probe.mock.calls[0][0] as { api_key: string; target_id: string };
    expect(body.api_key).toBe('viewer_secret_key');
    expect(body.target_id).toBe('metrics');
    // Only the one role that had a key is probed: a role with no key is left
    // unprobed rather than assumed either way.
    expect(probe).toHaveBeenCalledTimes(1);
  });

  it('never writes a key to storage', async () => {
    vi.spyOn(aiqaApi.rbac, 'probe').mockResolvedValue(result({}));
    render(<RbacProbeMatrix />);
    await screen.findByText('Prometheus metrics (deployment-wide counters)');
    fireEvent.change(screen.getByLabelText(/keys to probe with/i), {
      target: { value: 'viewer=super_secret_value' },
    });
    const row = rowFor('Prometheus metrics (deployment-wide counters)');
    fireEvent.click(within(row).getByRole('button', { name: /probe/i }));

    await waitFor(() =>
      expect(within(row).getByText('blocked by the role gate')).toBeTruthy(),
    );
    const dumped = JSON.stringify(window.localStorage);
    expect(dumped).not.toContain('super_secret_value');
    expect(dumped).not.toContain('viewer=');
  });

  it('does not report an aborted request as a load failure', async () => {
    // StrictMode mounts, unmounts and mounts again, so the first request is aborted
    // and its rejection can land after the second request has already succeeded.
    // Treating that as a failure renders "Couldn't load this data -- signal is
    // aborted without reason" over a grid that loaded perfectly well.
    //
    // This is the one bug in this file's history that a unit test missed and
    // driving the real page caught, so the test renders under StrictMode on
    // purpose: nothing else here double-mounts.
    let calls = 0;
    vi.spyOn(aiqaApi.rbac, 'probeTargets').mockImplementation((signal?: AbortSignal) => {
      calls += 1;
      if (calls === 1) {
        return new Promise((_resolve, reject) => {
          signal?.addEventListener('abort', () =>
            reject(
              new ApiError('not_found', 'signal is aborted without reason', null, null, null, null),
            ),
          );
        });
      }
      return Promise.resolve(TARGETS);
    });

    render(
      <React.StrictMode>
        <RbacProbeMatrix />
      </React.StrictMode>,
    );

    await screen.findByText('Prometheus metrics (deployment-wide counters)');
    expect(calls).toBeGreaterThan(1);
    expect(screen.queryByText(/aborted without reason/)).toBeNull();
    expect(screen.queryByText(/Couldn.t load/)).toBeNull();
    // The grid survived the aborted first request rather than being replaced by it.
    expect(screen.getAllByText('not probed').length).toBeGreaterThan(0);
  });

  it('never lets the sweep opt in to creating a real record', async () => {
    // Regression test for a bug that actually happened. The original claim was that
    // every mutating target addresses an id that cannot exist, so a probe that got
    // past the gate could not touch anything real. That held for every route with an
    // id in its path and was false for `POST /admin/api-keys` and
    // `POST /api/v1/projects`, which have none -- so "probe every route" with an
    // admin key minted four API keys and created four projects.
    //
    // The backend now refuses those two without a deliberate opt-in. This asserts
    // the page cannot turn that refusal off by accident, including by ticking the
    // box, because a sweep that creates records is never what anyone meant.
    const probe = vi.spyOn(aiqaApi.rbac, 'probe').mockResolvedValue(result({}));
    render(<RbacProbeMatrix />);
    await screen.findByText('Mint a new API key');

    // Tick the opt-in first, so the sweep is running with permission it must ignore.
    fireEvent.click(screen.getByRole('checkbox'));

    fireEvent.change(screen.getByLabelText(/keys to probe with/i), {
      target: { value: 'admin=admin_key' },
    });
    fireEvent.click(screen.getByRole('button', { name: /Probe every route/ }));

    await waitFor(() => expect(probe).toHaveBeenCalled());
    const sweepCalls = probe.mock.calls.filter(
      (c) => (c[0] as { target_id?: string }).target_id === 'keys-create',
    );
    expect(sweepCalls.length).toBe(1);
    for (const c of sweepCalls) {
      expect((c[0] as { allow_creation?: boolean }).allow_creation).toBeUndefined();
    }
  });

  it('says a creating target needs the opt-in, rather than claiming it is safe', async () => {
    render(<RbacProbeMatrix />);
    await screen.findByText('Mint a new API key');
    // `keys-create` creates a record. Its own row says so, and the checkbox states
    // the consequence in the same words rather than relying on a tooltip.
    expect(screen.getByText(/creates a real record if it gets past the gate/i)).toBeTruthy();
    expect(
      screen.getByRole('checkbox').closest('label')?.textContent ?? '',
    ).toMatch(/mint a real API key or create a real project/i);
  });

  it('surfaces a real 403 for probing itself rather than showing an empty grid', async () => {
    vi.spyOn(aiqaApi.rbac, 'probeTargets').mockRejectedValue(
      new ApiError(
        'forbidden',
        "role 'viewer' cannot perform this action (requires 'operator')",
        403,
        'FORBIDDEN',
        "role 'viewer' cannot perform this action (requires 'operator')",
        'req_rbac_probe_test',
      ),
    );
    render(<RbacProbeMatrix />);
    // The forbidden view renders the copy this page supplies, not the server's
    // detail string, so assert on the title the component was given.
    await waitFor(() => expect(screen.getByText(/Not permitted to probe roles/i)).toBeTruthy());
    // A refusal must be shown as a refusal. An empty grid instead would read as
    // "nothing to test", which is a different and wrong claim.
    expect(screen.queryByText('not probed')).toBeNull();
  });

  it('has no column anywhere that asserts a required role', async () => {
    render(<RbacProbeMatrix />);
    await screen.findByText('Prometheus metrics (deployment-wide counters)');
    // Two tables, one per category, so the headers repeat. Check one of them.
    const headers = screen
      .getAllByRole('columnheader')
      .slice(0, 6)
      .map((h) => h.textContent?.trim());
    // The last column carries an `sr-only` name rather than an empty header, so a
    // screen reader announces the probe button rather than nothing at all.
    expect(headers).toEqual([
      'Route',
      'What it does',
      'viewer',
      'operator',
      'admin',
      'Probe this route',
    ]);
    // The regression guard: a "requires" column is the thing that must not come
    // back, because a client-side role table is what this page stopped doing.
    expect(screen.queryByText(/required role|requires:/i)).toBeNull();
  });
});