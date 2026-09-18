/**
 * Phase 8: Telemetry Retention Modal Unit & Integration Tests
 */

import React from 'react';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TelemetryRetentionModal } from './TelemetryRetentionModal';
import { aiqaApi, ApiError } from '../../services/aiqaApi';
import type { RetentionReport, WhoAmIResponse } from '../../types/contract';

const ADMIN_ME: WhoAmIResponse = {
  name: 'admin-key',
  role: 'admin',
  project_id: null,
  role_hierarchy: ['viewer', 'operator', 'admin'],
};

const VIEWER_ME: WhoAmIResponse = {
  name: 'viewer-key',
  role: 'viewer',
  project_id: null,
  role_hierarchy: ['viewer', 'operator', 'admin'],
};

const MOCK_DRY_RUN_REPORT: RetentionReport = {
  project_id: 'proj-retention-test',
  eligible_count: 14,
  pruned_count: 0,
  dry_run: true,
  executed_at: '2026-09-16T12:00:00Z',
  retention_days: 30,
};

const MOCK_PRUNE_REPORT: RetentionReport = {
  project_id: 'proj-retention-test',
  eligible_count: 14,
  pruned_count: 14,
  dry_run: false,
  executed_at: '2026-09-16T12:05:00Z',
  retention_days: 30,
};

describe('TelemetryRetentionModal', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    // Default every test to a real admin identity — the existing tests
    // below exercise the "admin, fully enabled" path unchanged; the new
    // gap-closing tests further down override this per-test for the
    // non-admin and identity-check-failure cases.
    vi.spyOn(aiqaApi.rbac, 'whoami').mockResolvedValue(ADMIN_ME);
  });

  it('renders nothing when isOpen is false', () => {
    const { container } = render(
      <TelemetryRetentionModal
        projectId="proj-retention-test"
        isOpen={false}
        onClose={vi.fn()}
      />
    );
    expect(container.firstChild).toBeNull();
  });

  it('renders safety invariants and default retention window', async () => {
    render(
      <TelemetryRetentionModal
        projectId="proj-retention-test"
        isOpen={true}
        onClose={vi.fn()}
      />
    );
    await waitFor(() => expect(aiqaApi.rbac.whoami).toHaveBeenCalled());

    expect(screen.getByTestId('telemetry-retention-modal')).toBeDefined();
    expect(screen.getByText(/SAFETY INVARIANTS GUARANTEED/i)).toBeDefined();
    expect(screen.getByText(/Audit logs and Quality Graph nodes are 100% preserved/i)).toBeDefined();
    expect(screen.getByText(/Active runs \(PENDING or RUNNING\) are never pruned/i)).toBeDefined();

    const input = screen.getByTestId('retention-days-input') as HTMLInputElement;
    expect(input.value).toBe('30');
  });

  it('updates retention window input correctly', async () => {
    render(
      <TelemetryRetentionModal
        projectId="proj-retention-test"
        isOpen={true}
        onClose={vi.fn()}
      />
    );
    await waitFor(() => expect(aiqaApi.rbac.whoami).toHaveBeenCalled());

    const input = screen.getByTestId('retention-days-input') as HTMLInputElement;
    fireEvent.change(input, { target: { value: '45' } });
    expect(input.value).toBe('45');
  });

  it('triggers dry-run simulation and displays dry-run report', async () => {
    const dryRunSpy = vi
      .spyOn(aiqaApi.intelligence, 'retentionDryRun')
      .mockResolvedValue(MOCK_DRY_RUN_REPORT);

    render(
      <TelemetryRetentionModal
        projectId="proj-retention-test"
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    fireEvent.click(screen.getByTestId('retention-dry-run-button'));

    await waitFor(() => {
      expect(dryRunSpy).toHaveBeenCalledWith('proj-retention-test', { retention_days: 30 });
      expect(screen.getByTestId('retention-report')).toBeDefined();
    });

    expect(screen.getByText(/SIMULATION OUTCOME \(DRY-RUN\)/i)).toBeDefined();
    expect(screen.getByText('14')).toBeDefined();
    expect(screen.getByText(/No rows deleted/i)).toBeDefined();
  });

  it('triggers live prune with confirmation and displays prune report', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const pruneSpy = vi
      .spyOn(aiqaApi.intelligence, 'retentionPrune')
      .mockResolvedValue(MOCK_PRUNE_REPORT);

    render(
      <TelemetryRetentionModal
        projectId="proj-retention-test"
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    fireEvent.click(screen.getByTestId('retention-prune-button'));

    await waitFor(() => {
      expect(pruneSpy).toHaveBeenCalledWith('proj-retention-test', { retention_days: 30 });
      expect(screen.getByTestId('retention-report')).toBeDefined();
    });

    expect(screen.getByText(/PRUNE EXECUTED/i)).toBeDefined();
    expect(screen.getByText(/Selected historical telemetry rows have been permanently pruned/i)).toBeDefined();
  });

  it('handles simulation errors gracefully', async () => {
    vi.spyOn(aiqaApi.intelligence, 'retentionDryRun').mockRejectedValue(
      new Error('Role viewer cannot perform retention (requires admin)')
    );

    render(
      <TelemetryRetentionModal
        projectId="proj-retention-test"
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    fireEvent.click(screen.getByTestId('retention-dry-run-button'));

    await waitFor(() => {
      expect(screen.getByText(/requires admin/i)).toBeDefined();
    });
  });

  it('calls onClose when close button is clicked', async () => {
    const onClose = vi.fn();
    render(
      <TelemetryRetentionModal
        projectId="proj-retention-test"
        isOpen={true}
        onClose={onClose}
      />
    );
    await waitFor(() => expect(aiqaApi.rbac.whoami).toHaveBeenCalled());

    fireEvent.click(screen.getByTestId('close-retention-modal'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  describe('gap-closing — real role disclosure and disabled actions for a non-admin', () => {
    it('a real admin sees no role notice and both actions stay enabled', async () => {
      render(
        <TelemetryRetentionModal projectId="proj-retention-test" isOpen={true} onClose={vi.fn()} />
      );

      await waitFor(() => expect(aiqaApi.rbac.whoami).toHaveBeenCalled());
      expect(screen.queryByTestId('retention-role-notice')).toBeNull();
      expect((screen.getByTestId('retention-dry-run-button') as HTMLButtonElement).disabled).toBe(false);
      expect((screen.getByTestId('retention-prune-button') as HTMLButtonElement).disabled).toBe(false);
    });

    it('a real non-admin sees the real role disclosed upfront, with both actions disabled before any click', async () => {
      vi.spyOn(aiqaApi.rbac, 'whoami').mockResolvedValue(VIEWER_ME);
      const dryRunSpy = vi.spyOn(aiqaApi.intelligence, 'retentionDryRun');

      render(
        <TelemetryRetentionModal projectId="proj-retention-test" isOpen={true} onClose={vi.fn()} />
      );

      await waitFor(() => expect(screen.getByTestId('retention-role-notice')).toBeDefined());
      expect(screen.getByTestId('retention-role-notice').textContent).toContain('viewer');
      expect((screen.getByTestId('retention-dry-run-button') as HTMLButtonElement).disabled).toBe(true);
      expect((screen.getByTestId('retention-prune-button') as HTMLButtonElement).disabled).toBe(true);

      // A disabled button must not dispatch its handler — no wasted,
      // doomed-to-403 request is ever sent.
      fireEvent.click(screen.getByTestId('retention-dry-run-button'));
      expect(dryRunSpy).not.toHaveBeenCalled();
    });

    it('fails open when the identity check itself fails — never wrongly blocks a real admin over a transient hiccup', async () => {
      vi.spyOn(aiqaApi.rbac, 'whoami').mockRejectedValue(new Error('network blip'));

      render(
        <TelemetryRetentionModal projectId="proj-retention-test" isOpen={true} onClose={vi.fn()} />
      );

      await waitFor(() => expect(aiqaApi.rbac.whoami).toHaveBeenCalled());
      // No known role means no proactive block — the real backend gate on
      // the actual action call remains the authoritative boundary.
      expect(screen.queryByTestId('retention-role-notice')).toBeNull();
      expect((screen.getByTestId('retention-dry-run-button') as HTMLButtonElement).disabled).toBe(false);
    });

    it('a real 403 on the actual call is still handled honestly, independent of the proactive disable (defense in depth)', async () => {
      // The proactive check reports admin (so the button stays enabled —
      // this deliberately does NOT go through the disabled-button path),
      // but the real backend call itself still refuses with a genuine
      // 403 — e.g. a role change between the identity check and the
      // action. The real backend gate is the actual boundary; this proves
      // its refusal is shown with the same honest, role-aware copy either
      // way, not just when the proactive gate already caught it.
      vi.spyOn(aiqaApi.intelligence, 'retentionDryRun').mockRejectedValue(
        new ApiError('forbidden', "role 'viewer' cannot perform this action", 403),
      );

      render(
        <TelemetryRetentionModal projectId="proj-retention-test" isOpen={true} onClose={vi.fn()} />,
      );
      await waitFor(() => expect(aiqaApi.rbac.whoami).toHaveBeenCalled());
      expect((screen.getByTestId('retention-dry-run-button') as HTMLButtonElement).disabled).toBe(false);

      fireEvent.click(screen.getByTestId('retention-dry-run-button'));

      await waitFor(() =>
        expect(
          screen.getByText(/requires the Admin role — the orchestrator refused this with a real 403/),
        ).toBeDefined(),
      );
    });
  });
});
