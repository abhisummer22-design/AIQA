/**
 * Phase 7: QA Intelligence & Observability Dashboard Unit & Integration Tests
 * Comprehensive coverage of telemetry, clustering, flakiness, healing governance,
 * quality graph topology, planning insights, error handling, and state transitions.
 */

import React from 'react';
import { act, render, screen, waitFor, fireEvent, within } from '@testing-library/react';
import { beforeEach, afterEach, describe, expect, it, vi } from 'vitest';
import { QaIntelligenceDashboardPage } from './QaIntelligenceDashboardPage';
import { ProjectContext } from '../../context/ProjectContext';
import { aiqaApi, ApiError } from '../../services/aiqaApi';
import type {
  ExecutionTelemetryRow,
  FailureClusterRow,
  FlakinessMetricsRow,
  HealingStrategyMetricsRow,
  TrendAnalysis,
  QualityGraphResponse,
  RiskCalibration,
} from '../../types/contract';

const MOCK_PROJECT = {
  id: 'proj-qa-intel-001',
  name: 'Core Payment Service',
  tenant_id: 'tenant-acme',
  created_at: '2026-08-01T00:00:00Z',
  updated_at: '2026-08-01T00:00:00Z',
};

const MOCK_TELEMETRY: ExecutionTelemetryRow[] = [
  {
    id: 'tel-1',
    project_id: MOCK_PROJECT.id,
    run_id: 'run-alpha',
    task_id: 'task-checkout-flow',
    task_name: 'Checkout Flow E2E',
    test_case_id: 'tc-101',
    execution_id: 'exec-1',
    executor: 'Simulate',
    environment: 'staging',
    start_time: '2026-09-10T10:00:00Z',
    end_time: '2026-09-10T10:00:05Z',
    duration_ms: 5000,
    status: 'FAILED',
    retry_count: 1,
    healing_attempt_count: 1,
    failure_category: 'LOCATOR_TIMEOUT',
    failure_fingerprint: 'sig_btn_checkout_not_found',
    error_message: 'Element #btn-submit-order timed out after 3000ms. Token: aiqa_secret_key_to_redact',
    evidence_refs: JSON.stringify(['screenshot-1.png', 'network.har']),
    assertion_result: 'Expected element visible',
    http_status: 504,
    selector: '#btn-submit-order',
    verification_outcome: 'UNVERIFIED',
    created_at: '2026-09-10T10:00:06Z',
  },
  {
    id: 'tel-2',
    project_id: MOCK_PROJECT.id,
    run_id: 'run-alpha',
    task_id: 'task-auth-token',
    task_name: 'Auth Token Refresh',
    test_case_id: 'tc-102',
    execution_id: 'exec-2',
    executor: 'Simulate',
    environment: 'staging',
    start_time: '2026-09-10T10:01:00Z',
    end_time: '2026-09-10T10:01:01Z',
    duration_ms: 1200,
    status: 'PASSED',
    retry_count: 0,
    healing_attempt_count: 0,
    failure_category: null,
    failure_fingerprint: null,
    error_message: null,
    evidence_refs: '[]',
    assertion_result: 'Token refreshed successfully',
    http_status: 200,
    selector: null,
    verification_outcome: null,
    created_at: '2026-09-10T10:01:02Z',
  },
];

const MOCK_FAILURES: FailureClusterRow[] = [
  {
    id: 'fc-1',
    project_id: MOCK_PROJECT.id,
    signature: 'LOCATOR_TIMEOUT:#btn-submit-order',
    first_seen_at: '2026-09-08T10:00:00Z',
    last_seen_at: '2026-09-10T10:00:00Z',
    occurrence_count: 5,
    root_cause_hypothesis: 'DOM selector changed dynamically during checkout modal rendering.',
    confidence: 0.88,
    reproducibility: 0.95,
    review_state: 'CONFIRMED',
  },
];

const MOCK_FLAKINESS: FlakinessMetricsRow[] = [
  {
    id: 'flk-1',
    node_id: 'test-checkout-modal',
    flakiness_score: 0.35,
    window_runs: 20,
    window_failures: 7,
    environment_correlation: 'Staging: 85%, Production: 15%',
    computed_at: '2026-09-10T12:00:00Z',
  },
  {
    id: 'flk-2',
    node_id: 'test-user-login',
    flakiness_score: 0.05,
    window_runs: 50,
    window_failures: 1,
    environment_correlation: null,
    computed_at: '2026-09-10T12:00:00Z',
  },
];

const MOCK_STRATEGIES: HealingStrategyMetricsRow[] = [
  {
    id: 'strat-1',
    project_id: MOCK_PROJECT.id,
    strategy: 'AST Selector Fallback To TestID',
    failure_category: 'LOCATOR_TIMEOUT',
    attempts_count: 6,
    verified_count: 5,
    not_verified_count: 1,
    regression_count: 0,
    calibrated_confidence: 0.83,
    updated_at: '2026-09-10T12:00:00Z',
  },
];

const MOCK_TRENDS: TrendAnalysis = {
  metric_name: 'failure_rate',
  direction: 'IMPROVING',
  current_value: 0.12,
  previous_value: 0.28,
  sample_count: 15,
  notes: 'Failure rate decreased by 16% over the last 15 execution runs.',
};

const MOCK_QUALITY_GRAPH: QualityGraphResponse = {
  nodes: [
    {
      id: 'node-test-1',
      project_id: MOCK_PROJECT.id,
      node_type: 'test',
      external_ref: 'tests/e2e/checkout.spec.ts',
      name: 'E2E Checkout Test',
      metadata: JSON.stringify({ priority: 'P0', suite: 'e2e' }),
      created_at: '2026-09-01T00:00:00Z',
    },
    {
      id: 'node-task-1',
      project_id: MOCK_PROJECT.id,
      node_type: 'task',
      external_ref: null,
      name: 'Task: Run Checkout',
      metadata: JSON.stringify({ runner: 'playwright' }),
      created_at: '2026-09-01T00:00:00Z',
    },
    {
      id: 'node-cluster-1',
      project_id: MOCK_PROJECT.id,
      node_type: 'failure_cluster',
      external_ref: null,
      name: 'Cluster: LOCATOR_TIMEOUT',
      metadata: JSON.stringify({ occurrences: 5 }),
      created_at: '2026-09-08T00:00:00Z',
    },
  ],
  relations: [
    {
      id: 'rel-1',
      from_node_id: 'node-test-1',
      to_node_id: 'node-task-1',
      relation_type: 'EXECUTES_AS',
      metadata: '{}',
      created_at: '2026-09-01T00:00:00Z',
    },
    {
      id: 'rel-2',
      from_node_id: 'node-task-1',
      to_node_id: 'node-cluster-1',
      relation_type: 'FAILED_WITH',
      metadata: '{}',
      created_at: '2026-09-08T00:00:00Z',
    },
  ],
};

const MOCK_RISK: RiskCalibration = {
  task_name: MOCK_PROJECT.id,
  risk_score: 0.45,
  risk_level: 'MEDIUM',
  factors: {
    failure_rate: 0.50,
    flakiness_score: 0.35,
    failure_severity: 0.40,
    dependency_count: 2,
    healing_regression_rate: 0.0,
    security_sensitivity: 0.0,
  },
  explanation:
    'Risk score 0.45 (MEDIUM): failure_rate=0.50, flakiness=0.35, severity=0.40, deps=2, regression=0.00',
};

function renderDashboard(project = MOCK_PROJECT as any) {
  const projectContextValue = {
    selectedProject: project,
    projects: project ? [project] : [],
    selectProject: vi.fn(),
    loading: false,
    refreshProjects: vi.fn(),
  };

  return render(
    <ProjectContext.Provider value={projectContextValue as any}>
      <QaIntelligenceDashboardPage />
    </ProjectContext.Provider>
  );
}

describe('QaIntelligenceDashboardPage & Subcomponents (Phase 7)', () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(aiqaApi.intelligence, 'telemetry').mockResolvedValue(MOCK_TELEMETRY);
    vi.spyOn(aiqaApi.intelligence, 'failures').mockResolvedValue(MOCK_FAILURES);
    vi.spyOn(aiqaApi.intelligence, 'flakiness').mockResolvedValue(MOCK_FLAKINESS);
    vi.spyOn(aiqaApi.intelligence, 'strategies').mockResolvedValue(MOCK_STRATEGIES);
    vi.spyOn(aiqaApi.intelligence, 'trends').mockResolvedValue(MOCK_TRENDS);
    vi.spyOn(aiqaApi.intelligence, 'qualityGraph').mockResolvedValue(MOCK_QUALITY_GRAPH);
    vi.spyOn(aiqaApi.intelligence, 'risk').mockResolvedValue(MOCK_RISK);
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  // 1. Empty Project State
  it('renders "Select a project" empty state when no project is selected', () => {
    renderDashboard(null);
    expect(screen.getByText('Select a project')).toBeTruthy();
    expect(screen.queryByTestId('intelligence-refresh-btn')).toBeNull();
  });

  // 2. Error States: 401, 403, 500, Offline
  it('renders 401 authentication required error state when API returns 401', async () => {
    vi.spyOn(aiqaApi.intelligence, 'telemetry').mockRejectedValue(new ApiError('unauthorized', 'Unauthorized'));
    renderDashboard();
    await waitFor(() => {
      expect(screen.getByText('Authentication required')).toBeTruthy();
    });
  });

  it('renders 403 forbidden error state when API returns 403', async () => {
    vi.spyOn(aiqaApi.intelligence, 'telemetry').mockRejectedValue(new ApiError('forbidden', 'Forbidden'));
    renderDashboard();
    await waitFor(() => {
      expect(screen.getByText('Access Forbidden')).toBeTruthy();
    });
  });

  it('renders generic error state when API returns 500 server error', async () => {
    vi.spyOn(aiqaApi.intelligence, 'telemetry').mockRejectedValue(new ApiError('server_error', 'Database connection lost'));
    renderDashboard();
    await waitFor(() => {
      expect(screen.getByText("Couldn't load QA Intelligence data")).toBeTruthy();
      expect(screen.getByText('Database connection lost')).toBeTruthy();
    });
  });

  it('renders offline state when network fails', async () => {
    vi.spyOn(aiqaApi.intelligence, 'telemetry').mockRejectedValue(new ApiError('offline', 'Network error'));
    renderDashboard();
    await waitFor(() => {
      expect(screen.getByText('Orchestrator unreachable')).toBeTruthy();
    });
  });

  // 3. Overview Tab: Health KPIs
  it('renders Execution Health KPIs correctly from real data', async () => {
    renderDashboard();
    await waitFor(() => {
      expect(screen.getByText('Core Payment Service')).toBeTruthy();
    });

    expect(screen.getByText('Total Executions')).toBeTruthy();
    expect(screen.getByText('Pass Rate')).toBeTruthy();
    expect(screen.getByText('Failure Rate')).toBeTruthy();
    expect(screen.getByText('Flaky Tests')).toBeTruthy();
    expect(screen.getByText('Avg Duration')).toBeTruthy();
    expect(screen.getByText('Healing Attempts')).toBeTruthy();
    expect(screen.getByText('Verified Heals')).toBeTruthy();
    expect(screen.getByText('Regressions Detected')).toBeTruthy();

    // Specific values based on MOCK_TELEMETRY: 2 total, 1 pass, 1 fail = 50.0% (both pass & fail rates)
    expect(screen.getAllByText('50.0%').length).toBe(2);
  });

  it('handles empty/zero execution data in Health KPIs with "Insufficient data"', async () => {
    vi.spyOn(aiqaApi.intelligence, 'telemetry').mockResolvedValue([]);
    vi.spyOn(aiqaApi.intelligence, 'flakiness').mockResolvedValue([]);
    vi.spyOn(aiqaApi.intelligence, 'strategies').mockResolvedValue([]);
    renderDashboard();

    await waitFor(() => {
      const insufficient = screen.getAllByText('Insufficient data');
      expect(insufficient.length).toBeGreaterThan(0);
    });
  });

  // 4. Overview Tab: Trends Section
  it('renders Trends Section with directional badges and values', async () => {
    renderDashboard();
    await waitFor(() => {
      expect(screen.getByText('IMPROVING')).toBeTruthy();
      expect(screen.getByText(/Failure rate decreased by 16%/)).toBeTruthy();
      expect(screen.getByText('Sample window: 15 runs')).toBeTruthy();
    });
  });

  it('handles INSUFFICIENT_DATA trend gracefully', async () => {
    vi.spyOn(aiqaApi.intelligence, 'trends').mockResolvedValue({
      metric_name: 'flakiness',
      direction: 'INSUFFICIENT_DATA',
      current_value: 0.1,
      previous_value: 0.1,
      sample_count: 1,
      notes: 'Need more runs',
    });
    renderDashboard();

    await waitFor(() => {
      expect(screen.getByText('Insufficient Historical Samples')).toBeTruthy();
    });
  });

  // 5. Overview Tab: Failure Clusters Section
  it('renders Failure Clusters with signatures and occurrences', async () => {
    renderDashboard();
    await waitFor(() => {
      expect(screen.getByText('LOCATOR_TIMEOUT:#btn-submit-order')).toBeTruthy();
      expect(screen.getByText('CONFIRMED')).toBeTruthy();
      expect(screen.getByText(/DOM selector changed dynamically/)).toBeTruthy();
      expect(screen.getByText('88%')).toBeTruthy();
    });
  });

  it('renders clean empty state when no failure clusters exist', async () => {
    vi.spyOn(aiqaApi.intelligence, 'failures').mockResolvedValue([]);
    renderDashboard();
    await waitFor(() => {
      expect(screen.getByText(/No recurring failure clusters detected/)).toBeTruthy();
    });
  });

  // 6. Overview Tab: Planning Insights (NO Chain-of-thought)
  it('renders Planning Insights recommendations without exposing reasoning tokens', async () => {
    renderDashboard();
    await waitFor(() => {
      expect(screen.getByText(/Test Planning Agent Feedback/)).toBeTruthy();
      expect(screen.getByText('Fragile Tests (1)')).toBeTruthy();
      expect(screen.getByText('test-checkout-modal')).toBeTruthy();
      expect(screen.getByText('Fragile Selectors (1)')).toBeTruthy();
      expect(screen.getByText('#btn-submit-order')).toBeTruthy();
      expect(screen.getByText('P1 - High Recurrence Defect')).toBeTruthy();
    });

    // Verify absence of typical chain-of-thought tokens
    expect(screen.queryByText(/thinking/i)).toBeNull();
    expect(screen.queryByText(/chain of thought/i)).toBeNull();
  });

  // 7. Telemetry Tab & Table
  it('switches to Telemetry tab and displays searchable, filterable table', async () => {
    renderDashboard();
    await waitFor(() => expect(screen.getByText('Total Executions')).toBeTruthy());

    fireEvent.click(screen.getByTestId('tab-telemetry'));
    expect(screen.getByText('Execution Telemetry')).toBeTruthy();
    expect(screen.getByText('Checkout Flow E2E')).toBeTruthy();
    expect(screen.getByText('Auth Token Refresh')).toBeTruthy();

    // Test Search input
    const searchInput = screen.getByTestId('telemetry-search-input');
    fireEvent.change(searchInput, { target: { value: 'Auth Token' } });
    expect(screen.getByText('Auth Token Refresh')).toBeTruthy();
    expect(screen.queryByText('Checkout Flow E2E')).toBeNull();

    // Clear search
    fireEvent.change(searchInput, { target: { value: '' } });

    // Test Status Filter
    const statusFilter = screen.getByTestId('telemetry-status-filter');
    fireEvent.change(statusFilter, { target: { value: 'FAILED' } });
    expect(screen.getByText('Checkout Flow E2E')).toBeTruthy();
    expect(screen.queryByText('Auth Token Refresh')).toBeNull();
  });

  // 8. Telemetry Detail Modal
  it('opens and closes Telemetry Detail Modal when a row is clicked', async () => {
    renderDashboard();
    await waitFor(() => expect(screen.getByText('Total Executions')).toBeTruthy());
    fireEvent.click(screen.getByTestId('tab-telemetry'));

    // Click row
    fireEvent.click(screen.getByTestId('telemetry-row-tel-1'));
    await waitFor(() => {
      expect(screen.getByTestId('telemetry-detail-modal')).toBeTruthy();
      expect(screen.getByText('Failure Category: LOCATOR_TIMEOUT')).toBeTruthy();
      expect(screen.getByText('sig_btn_checkout_not_found')).toBeTruthy();
    });

    // Redaction check: verify secret token in error message is redacted
    const errorText = screen.getByText(/Element #btn-submit-order timed out/);
    expect(errorText.textContent).toContain('••••••••');
    expect(errorText.textContent).not.toContain('aiqa_secret_key_to_redact');

    // Close modal
    fireEvent.click(screen.getByTitle('Close'));
    expect(screen.queryByTestId('telemetry-detail-modal')).toBeNull();
  });

  // 9. Flakiness & Risk Tab
  it('switches to Flakiness & Risk tab and displays stability metrics', async () => {
    renderDashboard();
    await waitFor(() => expect(screen.getByText('Total Executions')).toBeTruthy());
    fireEvent.click(screen.getByTestId('tab-flakiness'));

    expect(screen.getByText('Flakiness Intelligence')).toBeTruthy();
    expect(screen.getByText('test-checkout-modal')).toBeTruthy();
    expect(screen.getByText('35.0% (Flaky)')).toBeTruthy();
    expect(screen.getByText('test-user-login')).toBeTruthy();
    expect(screen.getByText('5.0% (Stable)')).toBeTruthy();

    // Multi-factor Risk drilldown
    expect(screen.getByText('Quality Risk & Multi-Factor Drilldown')).toBeTruthy();
    expect(screen.getByText('Failure Rate Factor')).toBeTruthy();
    expect(screen.getByText('Flakiness Factor')).toBeTruthy();
    expect(screen.getByText('Failure Severity Factor')).toBeTruthy();
    expect(screen.getByText('Dependency Factor')).toBeTruthy();
    expect(screen.getByText('Regression Factor')).toBeTruthy();
    expect(screen.getByText('Security & SSRF Boundary')).toBeTruthy();
    expect(screen.getByTestId('composite-risk-badge')).toBeTruthy();
    expect(screen.getByTestId('composite-risk-badge').textContent).toBe('MEDIUM');
    expect(screen.getByTestId('authoritative-risk-score').textContent).toContain('45%');
  });

  // 10. Flaky Test Drilldown Modal
  it('opens Flaky Test Detail Modal with outcome sequence', async () => {
    renderDashboard();
    await waitFor(() => expect(screen.getByText('Total Executions')).toBeTruthy());
    fireEvent.click(screen.getByTestId('tab-flakiness'));

    await waitFor(() => expect(screen.getByTestId('flakiness-drilldown-flk-1')).toBeTruthy());
    fireEvent.click(screen.getByTestId('flakiness-drilldown-flk-1'));

    await waitFor(() => {
      const modal = screen.getByTestId('flaky-test-detail-modal');
      expect(modal).toBeTruthy();
      expect(within(modal).getByText('Test Node: test-checkout-modal')).toBeTruthy();
      expect(within(modal).getByText('Flakiness Score: 35%')).toBeTruthy();
      expect(within(modal).getByText('Staging: 85%, Production: 15%')).toBeTruthy();
      expect(within(modal).getByText('13 PASS / 7 FAIL')).toBeTruthy();
    });

    // Close modal
    fireEvent.click(screen.getByText('Close'));
    expect(screen.queryByTestId('flaky-test-detail-modal')).toBeNull();
  });

  // 11. Healing Governance Tab
  it('switches to Healing Governance tab and verifies strict 7-step governance banner', async () => {
    renderDashboard();
    await waitFor(() => expect(screen.getByText('Total Executions')).toBeTruthy());
    fireEvent.click(screen.getByTestId('tab-healing'));

    expect(screen.getByText('Healing Strategy Reliability & Governance')).toBeTruthy();
    expect(screen.getByTestId('healing-governance-flow')).toBeTruthy();

    // 7 distinct governance steps
    expect(screen.getByText('Intelligence')).toBeTruthy();
    expect(screen.getByText('Recommendation')).toBeTruthy();
    expect(screen.getByText('Plan Validation')).toBeTruthy();
    expect(screen.getByText('Policy Gate')).toBeTruthy();
    expect(screen.getByText('RBAC Check')).toBeTruthy();
    expect(screen.getByText('Approval')).toBeTruthy();
    expect(screen.getByText('Execution')).toBeTruthy();

    // Safety boundary note
    expect(screen.getByText(/Strict Human Gate Enforced/)).toBeTruthy();
    expect(screen.getByText(/No unverified autonomous code alteration is permitted/)).toBeTruthy();

    // Strategy row
    expect(screen.getByText('AST Selector Fallback To TestID')).toBeTruthy();
    expect(screen.getByText('83%')).toBeTruthy();
  });

  // 12. Quality Graph Tab
  it('switches to Quality Graph tab and inspects node relations', async () => {
    renderDashboard();
    await waitFor(() => expect(screen.getByText('Total Executions')).toBeTruthy());
    fireEvent.click(screen.getByTestId('tab-graph'));

    await waitFor(() => expect(screen.getByText('Quality Graph Knowledge Network')).toBeTruthy());
    expect(screen.getByText('3 nodes · 2 edges')).toBeTruthy();

    // Click node to open inspector
    fireEvent.click(screen.getByTestId('graph-node-node-test-1'));
    await waitFor(() => {
      const inspector = screen.getByTestId('graph-node-inspector');
      expect(inspector.textContent).toContain('E2E Checkout Test');
      expect(inspector.textContent).toContain('tests/e2e/checkout.spec.ts');
      expect(inspector.textContent).toContain('→ OUTGOING: EXECUTES_AS');
    });

    // Filter by type
    const typeFilter = screen.getByTestId('graph-type-filter');
    fireEvent.change(typeFilter, { target: { value: 'task' } });
    expect(screen.getByText('Task: Run Checkout')).toBeTruthy();
    expect(screen.queryByTestId('graph-node-node-test-1')).toBeNull();
  });

  // 13. Refresh Button
  it('clicking Refresh refetches all intelligence endpoints', async () => {
    renderDashboard();
    await waitFor(() => expect(screen.getByTestId('intelligence-refresh-btn')).toBeTruthy());

    const initialCalls = vi.mocked(aiqaApi.intelligence.telemetry).mock.calls.length;
    fireEvent.click(screen.getByTestId('intelligence-refresh-btn'));

    await waitFor(() => {
      expect(vi.mocked(aiqaApi.intelligence.telemetry).mock.calls.length).toBeGreaterThan(initialCalls);
    });
  });

  // 14. Authoritative Risk Intelligence Consistency
  it('displays backend-provided risk score and classification without client-side recalculation', async () => {
    // Custom backend risk payload that would fail under the old 40/30/30 formula
    const customRisk: RiskCalibration = {
      task_name: MOCK_PROJECT.id,
      risk_score: 0.72,
      risk_level: 'CRITICAL',
      factors: {
        failure_rate: 0.85,
        flakiness_score: 0.60,
        failure_severity: 0.80,
        dependency_count: 4,
        healing_regression_rate: 0.20,
        security_sensitivity: 0.0,
      },
      explanation: 'Risk score 0.72 (CRITICAL): Authoritative Phase 6 Model',
    };
    vi.spyOn(aiqaApi.intelligence, 'risk').mockResolvedValue(customRisk);

    renderDashboard();
    await waitFor(() => expect(screen.getByText('Total Executions')).toBeTruthy());
    fireEvent.click(screen.getByTestId('tab-flakiness'));

    // 1. Authoritative score displayed correctly
    const scoreBadge = screen.getByTestId('authoritative-risk-score');
    expect(scoreBadge.textContent).toContain('72%');
    expect(scoreBadge.textContent).toContain('(0.72)');

    // 2. Authoritative classification displayed correctly
    const riskBadge = screen.getByTestId('composite-risk-badge');
    expect(riskBadge.textContent).toBe('CRITICAL');

    // 3. Explanation displayed correctly
    expect(screen.getByTestId('risk-explanation').textContent).toBe(
      'Risk score 0.72 (CRITICAL): Authoritative Phase 6 Model'
    );

    // 4. Authoritative factors displayed without hardcoded weights
    expect(screen.getByText('85.0%')).toBeTruthy();
    expect(screen.getByText('60.0%')).toBeTruthy();
    expect(screen.getByText('80.0%')).toBeTruthy();
    expect(screen.getByText('4 relations')).toBeTruthy();
    expect(screen.getByText('20.0%')).toBeTruthy();
    expect(screen.getByText('PASSED')).toBeTruthy();
    expect(screen.queryByText('40% weight')).toBeNull();
    expect(screen.queryByText('30% weight')).toBeNull();
  });

  // 15. Missing Factor Data Safety
  it('handles missing risk data safely with INSUFFICIENT_DATA and no fabricated factor percentages', async () => {
    vi.spyOn(aiqaApi.intelligence, 'risk').mockResolvedValue(null);

    renderDashboard();
    await waitFor(() => expect(screen.getByText('Total Executions')).toBeTruthy());
    fireEvent.click(screen.getByTestId('tab-flakiness'));

    const riskBadge = screen.getByTestId('composite-risk-badge');
    expect(riskBadge.textContent).toBe('INSUFFICIENT_DATA');
    expect(screen.queryByTestId('authoritative-risk-score')).toBeNull();

    // Factor values are honest N/A, not fabricated numbers
    const naElements = screen.getAllByText('N/A');
    expect(naElements.length).toBeGreaterThanOrEqual(5);
  });
});
