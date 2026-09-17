/**
 * Phase 7: QA Intelligence & Observability Dashboard Page
 * Real-time production observability, telemetry drill-down, failure clustering,
 * flakiness tracking, healing safety governance, quality graph network, and planning insights.
 */

import React, { useState, useEffect, useCallback, useRef } from 'react';
import {
  Activity,
  RotateCcw,
  Loader2,
  FolderKanban,
  Layers,
  AlertTriangle,
  Wrench,
  Network,
  Sparkles,
  LayoutDashboard,
  Database,
} from 'lucide-react';
import { useProject } from '../../context/ProjectContext';
import { aiqaApi, ApiError } from '../../services/aiqaApi';
import { EmptyState } from '../common/EmptyState';
import { renderResourceErrorState, ResourceErrorCopy } from '../state/StateViews';
import type {
  ExecutionTelemetryRow,
  FailureClusterRow,
  FlakinessMetricsRow,
  HealingStrategyMetricsRow,
  TrendAnalysis,
  QualityGraphResponse,
  RiskCalibration,
} from '../../types/contract';

// Subcomponents
import { ExecutionHealthSection } from '../intelligence/ExecutionHealthSection';
import { TelemetryTableSection } from '../intelligence/TelemetryTableSection';
import { TelemetryDetailModal } from '../intelligence/TelemetryDetailModal';
import { FailureIntelligenceSection } from '../intelligence/FailureIntelligenceSection';
import { FlakinessSection } from '../intelligence/FlakinessSection';
import { HealingIntelligenceSection } from '../intelligence/HealingIntelligenceSection';
import { RiskIntelligenceSection } from '../intelligence/RiskIntelligenceSection';
import { TrendsSection } from '../intelligence/TrendsSection';
import { QualityGraphSection } from '../intelligence/QualityGraphSection';
import { PlanningInsightsSection } from '../intelligence/PlanningInsightsSection';
import { TelemetryRetentionModal } from '../intelligence/TelemetryRetentionModal';

type IntelligenceTab = 'overview' | 'telemetry' | 'flakiness' | 'healing' | 'graph';

const ERROR_COPY: ResourceErrorCopy = {
  forbiddenTitle: 'Access Forbidden',
  forbiddenDescription: 'You do not have permission to view QA Intelligence & Observability data for this project.',
  offlineDescriptionSuffix: 'to load QA intelligence data.',
  genericTitle: "Couldn't load QA Intelligence data",
};

export const QaIntelligenceDashboardPage: React.FC = () => {
  const { selectedProject } = useProject();
  const selectedProjectId = selectedProject?.id ?? null;

  const [activeTab, setActiveTab] = useState<IntelligenceTab>('overview');
  const [loading, setLoading] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  // Loaded data
  const [telemetry, setTelemetry] = useState<ExecutionTelemetryRow[]>([]);
  const [failures, setFailures] = useState<FailureClusterRow[]>([]);
  const [flakiness, setFlakiness] = useState<FlakinessMetricsRow[]>([]);
  const [strategies, setStrategies] = useState<HealingStrategyMetricsRow[]>([]);
  const [trends, setTrends] = useState<TrendAnalysis | null>(null);
  const [qualityGraph, setQualityGraph] = useState<QualityGraphResponse | null>(null);
  const [risk, setRisk] = useState<RiskCalibration | null>(null);

  // Selected telemetry row for modal
  const [selectedTelemetry, setSelectedTelemetry] = useState<ExecutionTelemetryRow | null>(null);
  const [retentionModalOpen, setRetentionModalOpen] = useState(false);

  const abortRef = useRef<AbortController | null>(null);

  const loadData = useCallback(async (projectId: string, isRefresh: boolean) => {
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    if (isRefresh) setRefreshing(true);
    else setLoading(true);
    setError(null);

    try {
      const [telData, failData, flkData, stratData, trendData, graphData, riskData] = await Promise.all([
        aiqaApi.intelligence.telemetry(projectId, controller.signal).catch((err) => {
          if (err instanceof ApiError && err.status === 404) return [];
          throw err;
        }),
        aiqaApi.intelligence.failures(projectId, controller.signal).catch((err) => {
          if (err instanceof ApiError && err.status === 404) return [];
          throw err;
        }),
        aiqaApi.intelligence.flakiness(projectId, controller.signal).catch((err) => {
          if (err instanceof ApiError && err.status === 404) return [];
          throw err;
        }),
        aiqaApi.intelligence.strategies(projectId, controller.signal).catch((err) => {
          if (err instanceof ApiError && err.status === 404) return [];
          throw err;
        }),
        aiqaApi.intelligence.trends(projectId, controller.signal).catch((err) => {
          if (err instanceof ApiError && err.status === 404) return null;
          throw err;
        }),
        aiqaApi.intelligence.qualityGraph(projectId, controller.signal).catch((err) => {
          if (err instanceof ApiError && err.status === 404) return { nodes: [], relations: [] };
          throw err;
        }),
        aiqaApi.intelligence.risk(projectId, controller.signal).catch((err) => {
          if (err instanceof ApiError && err.status === 404) return null;
          throw err;
        }),
      ]);

      if (controller.signal.aborted) return;

      setTelemetry(telData || []);
      setFailures(failData || []);
      setFlakiness(flkData || []);
      setStrategies(stratData || []);
      setTrends(trendData);
      setQualityGraph(graphData);
      setRisk(riskData);
    } catch (err) {
      if (err instanceof DOMException && err.name === 'AbortError') return;
      const apiErr = err instanceof ApiError ? err : new ApiError('server_error', String(err));
      setError(apiErr);
    } finally {
      if (!controller.signal.aborted) {
        setLoading(false);
        setRefreshing(false);
      }
    }
  }, []);

  useEffect(() => {
    if (!selectedProjectId) {
      abortRef.current?.abort();
      setTelemetry([]);
      setFailures([]);
      setFlakiness([]);
      setStrategies([]);
      setTrends(null);
      setQualityGraph(null);
      setRisk(null);
      setError(null);
      setLoading(false);
      setRefreshing(false);
      return;
    }
    void loadData(selectedProjectId, false);
  }, [selectedProjectId, loadData]);

  useEffect(() => () => abortRef.current?.abort(), []);

  const handleRefresh = () => {
    if (!selectedProjectId || loading || refreshing) return;
    void loadData(selectedProjectId, true);
  };

  // 1. If no project selected
  if (!selectedProject) {
    return (
      <div className="p-6 max-w-7xl mx-auto" data-testid="aiqa-intelligence-dashboard">
        <EmptyState
          icon={FolderKanban}
          title="Select a project"
          description="Choose a project from the top selector to inspect its QA Intelligence, execution telemetry, and observability health."
        />
      </div>
    );
  }

  // 2. If error loading
  if (error) {
    return (
      <div className="p-6 max-w-7xl mx-auto" data-testid="aiqa-intelligence-dashboard">
        {renderResourceErrorState(error, handleRefresh, ERROR_COPY)}
      </div>
    );
  }

  const tabs: { id: IntelligenceTab; label: string; icon: React.ComponentType<{ className?: string }> }[] = [
    { id: 'overview', label: 'Overview', icon: LayoutDashboard },
    { id: 'telemetry', label: 'Telemetry', icon: Activity },
    { id: 'flakiness', label: 'Flakiness & Risk', icon: AlertTriangle },
    { id: 'healing', label: 'Healing Governance', icon: Wrench },
    { id: 'graph', label: 'Quality Graph', icon: Network },
  ];

  return (
    <div className="p-4 sm:p-6 max-w-7xl mx-auto space-y-6" data-testid="aiqa-intelligence-dashboard">
      {/* Top Header Bar */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 border-b border-slate-800 pb-4">
        <div>
          <div className="flex items-center gap-2.5">
            <div className="p-2 rounded-lg bg-violet-500/10 border border-violet-500/20 text-violet-400">
              <Activity className="w-5 h-5" />
            </div>
            <div>
              <h1 className="text-lg font-bold text-slate-100 flex items-center gap-2">
                <span>QA Intelligence & Observability</span>
                <span className="text-[11px] font-mono px-2 py-0.5 rounded bg-violet-950/60 text-violet-300 border border-violet-800/60">
                  {selectedProject.name}
                </span>
              </h1>
              <p className="text-xs text-slate-400">
                Governed adaptive intelligence, real execution telemetry, and empirical quality graphs.
              </p>
            </div>
          </div>
        </div>

        <div className="flex items-center gap-2">
          <button
            onClick={() => setRetentionModalOpen(true)}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-xs font-medium text-amber-400 hover:text-amber-300 transition-colors"
            data-testid="retention-maintenance-btn"
            title="Telemetry Retention Maintenance"
          >
            <Database className="w-3.5 h-3.5 text-amber-400" />
            <span>Retention</span>
          </button>
          <button
            onClick={handleRefresh}
            disabled={loading || refreshing}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-slate-900 hover:bg-slate-800 border border-slate-800 text-xs font-medium text-slate-200 transition-colors disabled:opacity-50"
            data-testid="intelligence-refresh-btn"
            title="Refresh Intelligence Data"
          >
            <RotateCcw className={`w-3.5 h-3.5 ${refreshing ? 'animate-spin text-violet-400' : 'text-slate-400'}`} />
            <span>{refreshing ? 'Refreshing...' : 'Refresh'}</span>
          </button>
        </div>
      </div>

      {/* Tabs Navigation */}
      <div className="flex items-center gap-1 border-b border-slate-800 pb-px overflow-x-auto">
        {tabs.map((tab) => {
          const Icon = tab.icon;
          const isActive = activeTab === tab.id;
          return (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`flex items-center gap-2 px-3.5 py-2 text-xs font-medium rounded-t-lg transition-colors border-b-2 -mb-px whitespace-nowrap ${
                isActive
                  ? 'border-violet-500 text-violet-400 bg-violet-500/5 font-semibold'
                  : 'border-transparent text-slate-400 hover:text-slate-200 hover:bg-slate-900/40'
              }`}
              data-testid={`tab-${tab.id}`}
            >
              <Icon className="w-3.5 h-3.5" />
              <span>{tab.label}</span>
            </button>
          );
        })}
      </div>

      {/* Tab Contents */}
      {activeTab === 'overview' && (
        <div className="space-y-6" data-testid="tab-content-overview">
          {/* Top Execution Health KPIs */}
          <ExecutionHealthSection
            telemetry={telemetry}
            strategies={strategies}
            flakiness={flakiness}
            loading={loading}
          />

          {/* Trajectory Trends */}
          <TrendsSection trends={trends} loading={loading} />

          {/* Top Failure Clusters */}
          <FailureIntelligenceSection
            failures={failures}
            projectId={selectedProject.id}
            loading={loading}
          />

          {/* Planning Insights */}
          <PlanningInsightsSection
            telemetry={telemetry}
            flakiness={flakiness}
            failures={failures}
            loading={loading}
          />
        </div>
      )}

      {activeTab === 'telemetry' && (
        <div className="space-y-6" data-testid="tab-content-telemetry">
          <TelemetryTableSection
            telemetry={telemetry}
            onSelectTelemetry={(row) => setSelectedTelemetry(row)}
            loading={loading}
          />
        </div>
      )}

      {activeTab === 'flakiness' && (
        <div className="space-y-6" data-testid="tab-content-flakiness">
          <FlakinessSection flakiness={flakiness} loading={loading} />
          <RiskIntelligenceSection
            risk={risk}
            loading={loading}
          />
        </div>
      )}

      {activeTab === 'healing' && (
        <div className="space-y-6" data-testid="tab-content-healing">
          <HealingIntelligenceSection strategies={strategies} loading={loading} />
        </div>
      )}

      {activeTab === 'graph' && (
        <div className="space-y-6" data-testid="tab-content-graph">
          <QualityGraphSection graph={qualityGraph} loading={loading} />
        </div>
      )}

      {/* Telemetry Detail Drawer / Modal */}
      {selectedTelemetry && (
        <TelemetryDetailModal
          telemetry={selectedTelemetry}
          onClose={() => setSelectedTelemetry(null)}
        />
      )}

      {/* Telemetry Retention Modal */}
      {retentionModalOpen && (
        <TelemetryRetentionModal
          projectId={selectedProject.id}
          isOpen={retentionModalOpen}
          onClose={() => {
            setRetentionModalOpen(false);
            void loadData(selectedProject.id, true);
          }}
        />
      )}
    </div>
  );
};
