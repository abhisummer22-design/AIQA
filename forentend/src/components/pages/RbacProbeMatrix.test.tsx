/**
 * What these tests protect.
 *
 * `RbacProbeMatrix` renders a grid whose whole credibility rests on one thing:
 * every cell is a real answer the server gave, never a value the client decided.
 * The failure this page used to have was precisely the opposite -- a grid filled
 * in from a hardcoded table (FE-11) and later from source parsing (this file's
 * siblings, abandoned after two parsers disagreed: 66 versus 8 routes without a
 * role gate, and an `ai-usage` row claiming admin when the route answers a
 * viewer).
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
 *  - nothing is persisted: an API key must not reach localStorage from here.
 */

import React from 'react';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RbacProbeMatrix } from './RbacProbeMatrix';
import { aiqaApi, ApiError } from '../../services/aiqaApi';
import type {
  RbacProbeResult,
  RbacProbeTarget,
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
    },
    {
      id: 'keys-create',
      method: 'POST',
      path: '/admin/api-keys',
      capability: 'Mint a new API key',
      category: 'Keys',
      mutating: true,
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
    const notProbed = screen.getAllByText('not probed');
    expect(notProbed.length).toBe(2 * 3); // two targets x three roles
    expect(screen.queryByText(/allowed/i)).toBeNull();
    expect(screen.queryByText(/blocked/i)).toBeNull();
  });

  it('shows the server\'s own wording for a refusal, not the client\'s', async () => {
    vi.spyOn(aiqaApi.rbac, 'probe').mockResolvedValue(
      result({
        gate: 'scope_refused',
        gate_label: 'blocked by the project-scope gate',
        message: "key is scoped to project abc and cannot act on def",
      }),
    );

    render(<RbacProbeMatrix />);
    await screen.findByText('Prometheus metrics (deployment-wide counters)');
    fireEvent.change(screen.getByLabelText(/keys to probe with/i), {
      target: { value: 'viewer=aiqa_viewer\noperator=aiqa_operator\nadmin=aiqa_admin' },
    });

    const row = screen.getByText('Prometheus metrics (deployment-wide counters)').closest('tr')!;
    fireEvent.click(within(row).getByRole('button', { name: /probe/i }));

    await waitFor(() =>
      // The label the server sent, verbatim. If the client had its own vocabulary
      // this would read differently, which is what the assertion is for.
      expect(screen.getAllByText('blocked by the project-scope gate').length).toBe(3),
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
    const row = screen.getByText('Prometheus metrics (deployment-wide counters)').closest('tr')!;
    fireEvent.click(within(row).getByRole('button', { name: /probe/i }));

    await waitFor(() =>
      expect(screen.getAllByText('allowed by the gate, refused afterwards').length).toBe(1),
    );
    // The status is shown as 422, so a reader can see it was not a 2xx.
    expect(within(row).getByText('422')).toBeTruthy();
    // And the validation message that caused it is available, not swallowed.
    expect(screen.getByText(/missing field/)).toBeTruthy();
  });

  it('marks a mutating route as able to change something', async () => {
    render(<RbacProbeMatrix />);
    await screen.findByText('Mint a new API key');
    expect(screen.getByText(/can change something/i)).toBeTruthy();
  });

  it('probes with the key for that role, not the key the page is signed in with', async () => {
    const probe = vi.spyOn(aiqaApi.rbac, 'probe').mockResolvedValue(result({}));
    render(<RbacProbeMatrix />);
    await screen.findByText('Prometheus metrics (deployment-wide counters)');
    fireEvent.change(screen.getByLabelText(/keys to probe with/i), {
      target: { value: 'viewer=viewer_secret_key' },
    });
    const row = screen.getByText('Prometheus metrics (deployment-wide counters)').closest('tr')!;
    fireEvent.click(within(row).getByRole('button', { name: /probe/i }));

    await waitFor(() => expect(probe).toHaveBeenCalled());
    const body = probe.mock.calls[0][0] as { api_key: string; target_id: string };
    expect(body.api_key).toBe('viewer_secret_key');
    expect(body.target_id).toBe('metrics');
    // Only the one role that had a key is probed: a role with no key is left
    // unprobed rather than assumed.
    expect(probe).toHaveBeenCalledTimes(1);
  });

  it('never writes a key to storage', async () => {
    vi.spyOn(aiqaApi.rbac, 'probe').mockResolvedValue(result({}));
    render(<RbacProbeMatrix />);
    await screen.findByText('Prometheus metrics (deployment-wide counters)');
    fireEvent.change(screen.getByLabelText(/keys to probe with/i), {
      target: { value: 'viewer=super_secret_value' },
    });
    const row = screen.getByText('Prometheus metrics (deployment-wide counters)').closest('tr')!;
    fireEvent.click(within(row).getByRole('button', { name: /probe/i }));

    await waitFor(() =>
      expect(
        within(row).getByText('blocked by the role gate'),
      ).toBeTruthy(),
    );
    const dumped = JSON.stringify(window.localStorage);
    expect(dumped).not.toContain('super_secret_value');
    expect(dumped).not.toContain('viewer=');
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
    await waitFor(() =>
      expect(screen.getByText(/Not permitted to probe roles/i)).toBeTruthy(),
    );
    // A refusal must be shown as a refusal. Rendering an empty matrix instead
    // would read as "nothing to test", which is a different and wrong claim.
    expect(screen.queryByText('not probed')).toBeNull();
  });

  it('has no column anywhere that asserts a required role', async () => {
    render(<RbacProbeMatrix />);
    await screen.findByText('Prometheus metrics (deployment-wide counters)');
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