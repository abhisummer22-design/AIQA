/**
 * Phase 8: Failure Investigation Workspace Modal Unit & Integration Tests
 */

import React from 'react';
import { render, screen, waitFor, fireEvent } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FailureInvestigationWorkspaceModal } from './FailureInvestigationWorkspaceModal';
import { aiqaApi } from '../../services/aiqaApi';
import type { FailureInvestigation } from '../../types/contract';

const MOCK_INVESTIGATION: FailureInvestigation = {
  project_id: 'proj-test-123',
  cluster_id: 'cluster-abc-456',
  signature: 'Timeout: Waiting for locator "#btn-checkout"',
  failure_category: 'TIMEOUT',
  root_cause_hypothesis: 'High API latency on payment gateway causing DOM element render timeout',
  confidence: 0.92,
  reproducibility: 0.85,
  environment_matrix: {
    cluster_id: 'cluster-abc-456',
    signature: 'Timeout: Waiting for locator "#btn-checkout"',
    recurrence_state: 'PERSISTENT',
    cross_environment: true,
    total_occurrences: 8,
    environments: [
      {
        environment: 'staging',
        occurrences: 5,
        total_runs: 10,
        failure_rate: 0.5,
        first_seen: '2026-09-10T10:00:00Z',
        last_seen: '2026-09-12T12:00:00Z',
        affected_tests: ['CheckoutTest', 'PaymentTest'],
        affected_components: ['#btn-checkout'],
        trend: 'DEGRADING',
      },
      {
        environment: 'production',
        occurrences: 3,
        total_runs: 50,
        failure_rate: 0.06,
        first_seen: '2026-09-11T14:00:00Z',
        last_seen: '2026-09-12T16:00:00Z',
        affected_tests: ['CheckoutTest'],
        affected_components: ['#btn-checkout'],
        trend: 'STABLE',
      },
    ],
  },
  timeline: [
    {
      id: 'event-1',
      timestamp: '2026-09-10T10:00:00Z',
      event_type: 'FAILURE_OBSERVED',
      environment: 'staging',
      description: 'Failure observed in task "CheckoutTest" (timed out after 5000ms)',
      evidence_refs: ['evidence/screen1.png'],
    },
    {
      id: 'event-2',
      timestamp: '2026-09-11T11:00:00Z',
      event_type: 'HEALING_PROPOSED',
      environment: 'orchestrator',
      description: 'Healing proposed for task "CheckoutTest" (risk: LOW, strategy: timing_adjustment)',
      evidence_refs: ['evidence/screen1.png'],
    },
    {
      id: 'event-3',
      timestamp: '2026-09-11T11:05:00Z',
      event_type: 'HEALING_APPROVED',
      environment: 'governance',
      description: 'Healing proposal approved by lead_qa',
      evidence_refs: ['evidence/screen1.png'],
    },
    {
      id: 'event-4',
      timestamp: '2026-09-11T11:10:00Z',
      event_type: 'HEALING_EXECUTED',
      environment: 'executor',
      description: 'Healing proposal execution state transitioned to "EXECUTED"',
      evidence_refs: ['evidence/screen1.png'],
    },
    {
      id: 'event-5',
      timestamp: '2026-09-11T11:15:00Z',
      event_type: 'HEALING_VERIFIED',
      environment: 'verification',
      description: 'Healing verification completed with outcome "PASSED"',
      evidence_refs: ['evidence/screen1.png'],
    },
  ],
  affected_tests: ['CheckoutTest', 'PaymentTest'],
  affected_tasks: ['task-101', 'task-102'],
  affected_components: ['#btn-checkout'],
  healing_history: [
    {
      proposal_id: 'prop-001',
      task_id: 'task-101',
      strategy: 'timing_adjustment',
      risk_level: 'LOW',
      state: 'VERIFIED',
      verification_result: 'PASSED',
      regression_detected: false,
      ast_diff: '- wait(5000)\n+ wait(10000)',
      created_at: '2026-09-11T11:00:00Z',
    },
  ],
  regression_status: 'No related regression observed',
  non_causal_changes: [
    'Failure frequency increased after healing proposal prop-001 (2026-09-11 11:00:00 UTC) (correlation only; direct causality not confirmed)',
  ],
  evidence_refs: ['evidence/screen1.png'],
  recommendations: [
    'CRITICAL: Correlated failure observed in production environment; prioritize immediate diagnostic triage and quarantine.',
    'Cross-environment divergence detected across 2 environments (production, staging); verify environment configuration parity.',
    'Brittle DOM selector identified in components [#btn-checkout]; update locator strategy to semantic test-ids.',
  ],
};

describe('FailureInvestigationWorkspaceModal', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it('renders nothing when isOpen is false', () => {
    const { container } = render(
      <FailureInvestigationWorkspaceModal
        projectId="proj-test-123"
        clusterId="cluster-abc-456"
        isOpen={false}
        onClose={vi.fn()}
      />
    );
    expect(container.firstChild).toBeNull();
  });

  it('fetches investigation data and displays header and hypothesis', async () => {
    vi.spyOn(aiqaApi.intelligence, 'investigation').mockResolvedValue(MOCK_INVESTIGATION);
    const onClose = vi.fn();

    render(
      <FailureInvestigationWorkspaceModal
        projectId="proj-test-123"
        clusterId="cluster-abc-456"
        isOpen={true}
        onClose={onClose}
      />
    );

    expect(screen.getByTestId('investigation-loading')).toBeDefined();

    await waitFor(() => {
      expect(screen.getByTestId('failure-investigation-workspace-modal')).toBeDefined();
    });

    expect(screen.getByText(/Operational Failure Investigation/i)).toBeDefined();
    expect(screen.getByText('TIMEOUT')).toBeDefined();
    expect(screen.getByText(/State: PERSISTENT/i)).toBeDefined();
    expect(screen.getByText(/Cross-Environment Divergence/i)).toBeDefined();
    expect(screen.getByText(/High API latency on payment gateway/i)).toBeDefined();
    expect(screen.getByText(/92%/i)).toBeDefined();
  });

  it('switches to environment matrix tab and renders environment statistics', async () => {
    vi.spyOn(aiqaApi.intelligence, 'investigation').mockResolvedValue(MOCK_INVESTIGATION);

    render(
      <FailureInvestigationWorkspaceModal
        projectId="proj-test-123"
        clusterId="cluster-abc-456"
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId('investigation-matrix-view')).toBeDefined();
    });

    expect(screen.getByText('staging')).toBeDefined();
    expect(screen.getByText('production')).toBeDefined();
    expect(screen.getByText('5 / 10')).toBeDefined();
    expect(screen.getByText('3 / 50')).toBeDefined();
  });

  it('switches to timeline tab and renders chronological events with evidence', async () => {
    vi.spyOn(aiqaApi.intelligence, 'investigation').mockResolvedValue(MOCK_INVESTIGATION);

    render(
      <FailureInvestigationWorkspaceModal
        projectId="proj-test-123"
        clusterId="cluster-abc-456"
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId('investigation-tab-timeline')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('investigation-tab-timeline'));

    await waitFor(() => {
      expect(screen.getByTestId('investigation-timeline-view')).toBeDefined();
    });

    expect(screen.getByText('FAILURE_OBSERVED')).toBeDefined();
    expect(screen.getByText('HEALING_PROPOSED')).toBeDefined();
    expect(screen.getByText('HEALING_APPROVED')).toBeDefined();
    expect(screen.getByText('HEALING_EXECUTED')).toBeDefined();
    expect(screen.getByText('HEALING_VERIFIED')).toBeDefined();
    expect(screen.getAllByText('evidence/screen1.png').length).toBeGreaterThan(0);
  });

  it('switches to healing history tab and toggles AST diff', async () => {
    vi.spyOn(aiqaApi.intelligence, 'investigation').mockResolvedValue(MOCK_INVESTIGATION);

    render(
      <FailureInvestigationWorkspaceModal
        projectId="proj-test-123"
        clusterId="cluster-abc-456"
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId('investigation-tab-healing')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('investigation-tab-healing'));

    await waitFor(() => {
      expect(screen.getByTestId('investigation-healing-view')).toBeDefined();
    });

    expect(screen.getByText(/Regression Assessment: No related regression observed/i)).toBeDefined();
    expect(screen.getByText(/Proposal: prop-00/i)).toBeDefined();

    const diffBtn = screen.getByText('View Proposed AST Diff');
    fireEvent.click(diffBtn);

    expect(screen.getByText(/- wait\(5000\)/i)).toBeDefined();
    expect(screen.getByText(/\+ wait\(10000\)/i)).toBeDefined();
  });

  it('switches to recommendations tab and renders non-causal observations and QA priorities', async () => {
    vi.spyOn(aiqaApi.intelligence, 'investigation').mockResolvedValue(MOCK_INVESTIGATION);

    render(
      <FailureInvestigationWorkspaceModal
        projectId="proj-test-123"
        clusterId="cluster-abc-456"
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId('investigation-tab-recommendations')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('investigation-tab-recommendations'));

    await waitFor(() => {
      expect(screen.getByTestId('investigation-recommendations-view')).toBeDefined();
    });

    expect(screen.getByText(/Temporal Change Correlations/i)).toBeDefined();
    expect(screen.getByText(/correlation only; direct causality not confirmed/i)).toBeDefined();
    expect(screen.getByText(/CRITICAL: Correlated failure observed in production/i)).toBeDefined();
  });

  it('handles API errors gracefully', async () => {
    vi.spyOn(aiqaApi.intelligence, 'investigation').mockRejectedValue(new Error('Internal server timeout'));

    render(
      <FailureInvestigationWorkspaceModal
        projectId="proj-test-123"
        clusterId="cluster-abc-456"
        isOpen={true}
        onClose={vi.fn()}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId('investigation-error')).toBeDefined();
    });

    expect(screen.getByText(/Internal server timeout/i)).toBeDefined();
  });

  it('calls onClose when close button is clicked', async () => {
    vi.spyOn(aiqaApi.intelligence, 'investigation').mockResolvedValue(MOCK_INVESTIGATION);
    const onClose = vi.fn();

    render(
      <FailureInvestigationWorkspaceModal
        projectId="proj-test-123"
        clusterId="cluster-abc-456"
        isOpen={true}
        onClose={onClose}
      />
    );

    await waitFor(() => {
      expect(screen.getByTestId('close-investigation-modal')).toBeDefined();
    });

    fireEvent.click(screen.getByTestId('close-investigation-modal'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
