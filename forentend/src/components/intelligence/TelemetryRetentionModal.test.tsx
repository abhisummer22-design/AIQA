/**
 * Phase 8: Telemetry Retention Modal Unit & Integration Tests
 */

import React from 'react';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { TelemetryRetentionModal } from './TelemetryRetentionModal';
import { aiqaApi } from '../../services/aiqaApi';
import type { RetentionReport } from '../../types/contract';

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

  it('renders safety invariants and default retention window', () => {
    render(
      <TelemetryRetentionModal
        projectId="proj-retention-test"
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    expect(screen.getByTestId('telemetry-retention-modal')).toBeDefined();
    expect(screen.getByText(/SAFETY INVARIANTS GUARANTEED/i)).toBeDefined();
    expect(screen.getByText(/Audit logs and Quality Graph nodes are 100% preserved/i)).toBeDefined();
    expect(screen.getByText(/Active runs \(PENDING or RUNNING\) are never pruned/i)).toBeDefined();

    const input = screen.getByTestId('retention-days-input') as HTMLInputElement;
    expect(input.value).toBe('30');
  });

  it('updates retention window input correctly', () => {
    render(
      <TelemetryRetentionModal
        projectId="proj-retention-test"
        isOpen={true}
        onClose={vi.fn()}
      />
    );

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

  it('calls onClose when close button is clicked', () => {
    const onClose = vi.fn();
    render(
      <TelemetryRetentionModal
        projectId="proj-retention-test"
        isOpen={true}
        onClose={onClose}
      />
    );

    fireEvent.click(screen.getByTestId('close-retention-modal'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
