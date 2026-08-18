/**
 * AIQA Orchestrator Page
 * Create test orchestration plans, route work, prioritize by risk/change/test value/dependencies,
 * and coordinate specialized capabilities across worker pools.
 */

import React, { useState } from 'react';
import {
  Workflow,
  Sparkles,
  GitBranch,
  Shield,
  Zap,
  Activity,
  Layers,
  CheckCircle2,
  Clock,
  Play,
  Filter,
  Plus,
  RefreshCw,
  Server,
  ArrowRight,
  TrendingUp,
  Sliders,
  ChevronRight,
  AlertTriangle,
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { SectionHeader } from '../common/SectionHeader';
import { StatusBadge } from '../common/StatusBadge';
import { OrchestratorPlan, AIQAStatus } from '../../types';

export const AIQAOrchestratorPage: React.FC = () => {
  const { runs, triggerNewRun, addActivity } = useAIQA();

  const [plans, setPlans] = useState<OrchestratorPlan[]>([
    {
      id: 'PLAN-901',
      name: 'Release Gating — Core Checkout & Ledger Microservices',
      scope: 'Full Service Boundary (PR-2489)',
      riskScore: 84,
      changeDeltaImpact: 'CRITICAL',
      testValueScore: 92,
      status: 'RUNNING',
      createdAt: '2026-08-16 09:15 UTC',
      specializedAgents: [
        { capability: 'SECURITY_AUDIT', status: 'ACTIVE', assignedWorkers: 4, testsHandled: 28 },
        { capability: 'API_CONTRACT', status: 'ACTIVE', assignedWorkers: 6, testsHandled: 74 },
        { capability: 'RBAC_MATRIX', status: 'COMPLETED', assignedWorkers: 2, testsHandled: 42 },
        { capability: 'LOAD_CHAOS', status: 'ACTIVE', assignedWorkers: 3, testsHandled: 15 },
        { capability: 'MOBILE_DEVICE', status: 'STANDBY', assignedWorkers: 1, testsHandled: 12 },
        { capability: 'E2E_SYNTHETIC', status: 'ACTIVE', assignedWorkers: 4, testsHandled: 36 },
      ],
      dagNodes: [
        { id: 'dag-1', label: '1. Ingest Specs & Schema Contracts', type: 'STAGE', status: 'PASSED', dependencies: [], durationEst: '12s' },
        { id: 'dag-2', label: '2. RBAC Access Path Validation', type: 'SUITE', status: 'PASSED', dependencies: ['dag-1'], durationEst: '24s' },
        { id: 'dag-3', label: '3. API Contract & Schema Verification', type: 'SUITE', status: 'RUNNING', dependencies: ['dag-1'], durationEst: '45s' },
        { id: 'dag-4', label: '4. Critical Path Synthetic E2E', type: 'SUITE', status: 'RUNNING', dependencies: ['dag-2'], durationEst: '68s' },
        { id: 'dag-5', label: '5. Chaos & Network Resiliency Fault Injection', type: 'SUITE', status: 'QUEUED', dependencies: ['dag-3', 'dag-4'], durationEst: '55s' },
        { id: 'dag-6', label: '6. Release Quality Gate Assessment', type: 'GATE', status: 'QUEUED', dependencies: ['dag-5'], durationEst: '5s' },
      ],
    },
    {
      id: 'PLAN-902',
      name: 'Mobile Core & Offline Sync Resilience',
      scope: 'Mobile Client v4.8 Beta',
      riskScore: 56,
      changeDeltaImpact: 'MEDIUM',
      testValueScore: 78,
      status: 'PASSED',
      createdAt: '2026-08-15 18:30 UTC',
      specializedAgents: [
        { capability: 'MOBILE_DEVICE', status: 'COMPLETED', assignedWorkers: 4, testsHandled: 34 },
        { capability: 'API_CONTRACT', status: 'COMPLETED', assignedWorkers: 2, testsHandled: 22 },
        { capability: 'SECURITY_AUDIT', status: 'COMPLETED', assignedWorkers: 2, testsHandled: 16 },
      ],
      dagNodes: [
        { id: 'dag-m1', label: 'Device Allocation & Provisioning', type: 'STAGE', status: 'PASSED', dependencies: [], durationEst: '15s' },
        { id: 'dag-m2', label: 'Deep Link & Biometric Token Auth', type: 'SUITE', status: 'PASSED', dependencies: ['dag-m1'], durationEst: '32s' },
        { id: 'dag-m3', label: 'Offline DB Sync & Background Drain', type: 'SUITE', status: 'PASSED', dependencies: ['dag-m2'], durationEst: '48s' },
        { id: 'dag-m4', label: 'Telemetry & Gate Approval', type: 'GATE', status: 'PASSED', dependencies: ['dag-m3'], durationEst: '4s' },
      ],
    },
  ]);

  const [selectedPlanId, setSelectedPlanId] = useState<string>('PLAN-901');
  const [priorityFilter, setPriorityFilter] = useState<'ALL' | 'CRITICAL' | 'HIGH' | 'MEDIUM'>('ALL');
  const [isCreatingPlan, setIsCreatingPlan] = useState(false);
  const [newPlanName, setNewPlanName] = useState('');
  const [newPlanScope, setNewPlanScope] = useState('Payment Gateway & Auth v2');
  const [newPlanRisk, setNewPlanRisk] = useState<'CRITICAL' | 'HIGH' | 'MEDIUM'>('HIGH');

  const selectedPlan = plans.find((p) => p.id === selectedPlanId) || plans[0];

  const handleCreatePlan = (e: React.FormEvent) => {
    e.preventDefault();
    if (!newPlanName.trim()) return;

    const newPlan: OrchestratorPlan = {
      id: `PLAN-${Math.floor(100 + Math.random() * 900)}`,
      name: newPlanName,
      scope: newPlanScope,
      riskScore: newPlanRisk === 'CRITICAL' ? 92 : newPlanRisk === 'HIGH' ? 76 : 48,
      changeDeltaImpact: newPlanRisk,
      testValueScore: 88,
      status: 'QUEUED',
      createdAt: new Date().toISOString().replace('T', ' ').substring(0, 16) + ' UTC',
      specializedAgents: [
        { capability: 'SECURITY_AUDIT', status: 'ACTIVE', assignedWorkers: 3, testsHandled: 18 },
        { capability: 'API_CONTRACT', status: 'ACTIVE', assignedWorkers: 4, testsHandled: 45 },
        { capability: 'RBAC_MATRIX', status: 'STANDBY', assignedWorkers: 2, testsHandled: 20 },
        { capability: 'LOAD_CHAOS', status: 'STANDBY', assignedWorkers: 2, testsHandled: 10 },
      ],
      dagNodes: [
        { id: `dag-${Date.now()}-1`, label: '1. Dependency Analysis & Code Delta Scan', type: 'STAGE', status: 'QUEUED', dependencies: [], durationEst: '10s' },
        { id: `dag-${Date.now()}-2`, label: '2. High-Risk Contract & Security Scan', type: 'SUITE', status: 'QUEUED', dependencies: [`dag-${Date.now()}-1`], durationEst: '35s' },
        { id: `dag-${Date.now()}-3`, label: '3. API Validation & Chaos Injection', type: 'SUITE', status: 'QUEUED', dependencies: [`dag-${Date.now()}-2`], durationEst: '50s' },
        { id: `dag-${Date.now()}-4`, label: '4. Autonomous Policy Gate Decision', type: 'GATE', status: 'QUEUED', dependencies: [`dag-${Date.now()}-3`], durationEst: '5s' },
      ],
    };

    setPlans([newPlan, ...plans]);
    setSelectedPlanId(newPlan.id);
    setIsCreatingPlan(false);
    setNewPlanName('');

    addActivity({
      type: 'RUN_STARTED',
      title: `Orchestrator Plan Created: ${newPlan.id}`,
      description: `Plan "${newPlan.name}" initialized with ${newPlan.specializedAgents.length} specialized agents.`,
      actor: 'AIQA Orchestrator',
    });
  };

  const handleExecutePlan = (planId: string) => {
    setPlans(
      plans.map((p) => {
        if (p.id === planId) {
          return {
            ...p,
            status: 'RUNNING',
            dagNodes: p.dagNodes.map((n, idx) => ({
              ...n,
              status: idx === 0 ? 'RUNNING' : 'QUEUED',
            })),
          };
        }
        return p;
      })
    );

    triggerNewRun({
      title: `Orchestrated Plan Execution: ${selectedPlan.name}`,
      environment: 'staging',
      executor: 'k8s-runner-pool-us',
      concurrency: 16,
    });
  };

  return (
    <div className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="AIQA Orchestrator"
        subtitle="Create orchestration plans, route work across specialized agent nodes, prioritize by risk/change/value, and coordinate DAG dependencies"
        actions={
          <div className="flex items-center gap-2">
            <button
              onClick={() => setIsCreatingPlan(!isCreatingPlan)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer"
            >
              <Plus className="w-3.5 h-3.5" />
              <span>Create Plan</span>
            </button>
            <button
              onClick={() => handleExecutePlan(selectedPlan.id)}
              disabled={selectedPlan.status === 'RUNNING'}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] hover:bg-slate-800 text-slate-200 text-xs font-semibold border border-slate-800 cursor-pointer disabled:opacity-50"
            >
              <Play className="w-3.5 h-3.5 text-emerald-400" />
              <span>Dispatch Selected Plan</span>
            </button>
          </div>
        }
      />

      {/* Plan Creator Modal / Panel */}
      {isCreatingPlan && (
        <form
          onSubmit={handleCreatePlan}
          className="p-4 rounded-lg border border-indigo-500/40 bg-[#0B1120] space-y-3"
        >
          <div className="flex items-center justify-between pb-2 border-b border-slate-800">
            <div className="flex items-center gap-2">
              <Sparkles className="w-4 h-4 text-indigo-400" />
              <h3 className="text-xs font-bold text-slate-100 uppercase tracking-wider font-mono">
                Create AIQA Orchestration Plan
              </h3>
            </div>
            <button
              type="button"
              onClick={() => setIsCreatingPlan(false)}
              className="text-xs text-slate-400 hover:text-slate-200"
            >
              Cancel
            </button>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-3 gap-3 text-xs">
            <div>
              <label className="block text-slate-300 font-medium mb-1">Plan Name</label>
              <input
                type="text"
                value={newPlanName}
                onChange={(e) => setNewPlanName(e.target.value)}
                placeholder="e.g. End-of-Sprint Full Regression & Security Gating"
                className="w-full px-2.5 py-1.5 rounded bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none"
                required
              />
            </div>
            <div>
              <label className="block text-slate-300 font-medium mb-1">Target Scope / Module</label>
              <input
                type="text"
                value={newPlanScope}
                onChange={(e) => setNewPlanScope(e.target.value)}
                className="w-full px-2.5 py-1.5 rounded bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono"
              />
            </div>
            <div>
              <label className="block text-slate-300 font-medium mb-1">Change Delta Impact</label>
              <select
                value={newPlanRisk}
                onChange={(e) => setNewPlanRisk(e.target.value as any)}
                className="w-full px-2.5 py-1.5 rounded bg-[#020617] border border-slate-800 text-slate-200 focus:border-indigo-500 focus:outline-none font-mono"
              >
                <option value="CRITICAL">CRITICAL — High Code Churn</option>
                <option value="HIGH">HIGH — Cross-Service APIs</option>
                <option value="MEDIUM">MEDIUM — Minor Delta</option>
              </select>
            </div>
          </div>

          <div className="flex justify-end gap-2 pt-2 border-t border-slate-800">
            <button
              type="submit"
              className="px-4 py-1.5 rounded bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs cursor-pointer"
            >
              Generate & Schedule Orchestration DAG
            </button>
          </div>
        </form>
      )}

      {/* Top Metrics Row: Orchestrator Engine Overview */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-2.5">
        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Active Orchestrations</span>
            <Workflow className="w-3.5 h-3.5 text-indigo-400" />
          </div>
          <div className="text-lg font-bold font-mono text-slate-100">
            {plans.filter((p) => p.status === 'RUNNING').length} Active / {plans.length} Total
          </div>
          <div className="text-[10px] text-slate-500">Autonomous DAG routing enabled</div>
        </div>

        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Risk-Weighted Priority</span>
            <AlertTriangle className="w-3.5 h-3.5 text-rose-400" />
          </div>
          <div className="text-lg font-bold font-mono text-rose-300">
            {selectedPlan.riskScore}/100 Risk Index
          </div>
          <div className="text-[10px] text-slate-500">
            Impact Level: <strong className="text-slate-300">{selectedPlan.changeDeltaImpact}</strong>
          </div>
        </div>

        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Test Value ROI</span>
            <TrendingUp className="w-3.5 h-3.5 text-emerald-400" />
          </div>
          <div className="text-lg font-bold font-mono text-emerald-400">
            {selectedPlan.testValueScore}% High-Yield
          </div>
          <div className="text-[10px] text-slate-500">Eliminating redundant test loops</div>
        </div>

        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Specialized Agents</span>
            <Server className="w-3.5 h-3.5 text-cyan-400" />
          </div>
          <div className="text-lg font-bold font-mono text-cyan-300">
            {selectedPlan.specializedAgents.reduce((acc, a) => acc + a.assignedWorkers, 0)} Worker Nodes
          </div>
          <div className="text-[10px] text-slate-500">Coordinated capability clusters</div>
        </div>
      </div>

      {/* Main Orchestrator Grid: Plan Selector & DAG Visualizer */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-4">
        {/* Left Column: Plan Selectors & Priority Routing (4 cols) */}
        <div className="lg:col-span-4 rounded-lg border border-slate-800 bg-[#0B1120] p-3 space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-slate-800">
            <span className="text-xs font-bold text-slate-200 uppercase font-mono">Orchestrator Plans</span>
            <span className="text-[10px] font-mono text-slate-500">DAG Schedulers</span>
          </div>

          <div className="space-y-2">
            {plans.map((plan) => {
              const isSelected = plan.id === selectedPlanId;
              return (
                <div
                  key={plan.id}
                  onClick={() => setSelectedPlanId(plan.id)}
                  className={`p-2.5 rounded-md border text-xs cursor-pointer transition-all ${
                    isSelected
                      ? 'border-indigo-500 bg-indigo-950/40 text-slate-100 shadow-sm'
                      : 'border-slate-800 bg-[#020617] hover:border-slate-700 text-slate-300'
                  }`}
                >
                  <div className="flex items-center justify-between gap-1 mb-1">
                    <span className="font-mono text-xs font-bold text-indigo-400">{plan.id}</span>
                    <StatusBadge status={plan.status} size="sm" />
                  </div>
                  <div className="font-semibold text-slate-100 truncate text-[11px] mb-1">
                    {plan.name}
                  </div>
                  <div className="flex items-center justify-between text-[10px] font-mono text-slate-400">
                    <span>Scope: {plan.scope}</span>
                    <span className="text-rose-400">Risk: {plan.riskScore}</span>
                  </div>
                </div>
              );
            })}
          </div>

          {/* Specialized Agent Coordination Matrix */}
          <div className="pt-3 border-t border-slate-800 space-y-2">
            <div className="text-[10px] font-mono uppercase tracking-wider text-slate-400 font-bold flex items-center justify-between">
              <span>Specialized Capabilities</span>
              <span className="text-indigo-400">Routing</span>
            </div>
            <div className="space-y-1.5">
              {selectedPlan.specializedAgents.map((agent, i) => (
                <div
                  key={i}
                  className="p-2 rounded bg-[#020617] border border-slate-800 flex items-center justify-between text-[11px]"
                >
                  <div className="space-y-0.5">
                    <div className="font-mono font-semibold text-slate-200">
                      {agent.capability.replace('_', ' ')}
                    </div>
                    <div className="text-[10px] text-slate-400 font-mono">
                      {agent.testsHandled} test vectors assigned
                    </div>
                  </div>
                  <div className="text-right">
                    <span
                      className={`text-[9px] font-mono px-1.5 py-0.2 rounded font-semibold ${
                        agent.status === 'ACTIVE'
                          ? 'bg-emerald-950 text-emerald-300 border border-emerald-800'
                          : agent.status === 'COMPLETED'
                          ? 'bg-slate-800 text-slate-300'
                          : 'bg-indigo-950 text-indigo-300 border border-indigo-800'
                      }`}
                    >
                      {agent.status} ({agent.assignedWorkers}w)
                    </span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>

        {/* Right Column: Execution DAG & Dependency Routing (8 cols) */}
        <div className="lg:col-span-8 rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-4">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-slate-800">
            <div>
              <div className="flex items-center gap-2">
                <span className="font-mono font-bold text-xs text-indigo-400">{selectedPlan.id}</span>
                <h3 className="text-sm font-bold text-slate-100">{selectedPlan.name}</h3>
              </div>
              <p className="text-xs text-slate-400 mt-0.5 font-mono">
                Target: {selectedPlan.scope} • Created: {selectedPlan.createdAt}
              </p>
            </div>
            <StatusBadge status={selectedPlan.status} />
          </div>

          {/* DAG Visualizer Pipeline */}
          <div className="space-y-2.5">
            <div className="flex items-center justify-between text-xs font-mono text-slate-400">
              <span className="font-bold text-slate-300 uppercase">Dependency DAG Stages</span>
              <span>Autonomous Dependency Resolution</span>
            </div>

            <div className="space-y-2">
              {selectedPlan.dagNodes.map((node, index) => (
                <div
                  key={node.id}
                  className="p-3 rounded-lg bg-[#020617] border border-slate-800 flex flex-col sm:flex-row sm:items-center justify-between gap-3 text-xs"
                >
                  <div className="flex items-center gap-3">
                    <div className="w-6 h-6 rounded bg-[#0B1120] border border-slate-700 flex items-center justify-center font-mono font-bold text-indigo-400 text-[11px] shrink-0">
                      {index + 1}
                    </div>
                    <div className="space-y-0.5">
                      <div className="font-semibold text-slate-200 flex items-center gap-2">
                        <span>{node.label}</span>
                        <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-slate-800 text-slate-400 border border-slate-700">
                          {node.type}
                        </span>
                      </div>
                      <div className="text-[10px] font-mono text-slate-500">
                        {node.dependencies.length > 0
                          ? `Depends on: ${node.dependencies.join(', ')}`
                          : 'Root dependency (Immediate)'}
                      </div>
                    </div>
                  </div>

                  <div className="flex items-center gap-3 font-mono text-xs">
                    <span className="text-slate-400 text-[11px]">Est: {node.durationEst}</span>
                    <StatusBadge status={node.status} size="sm" />
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Prioritization Logic & Risk Weights Breakdown */}
          <div className="p-3 rounded-lg bg-[#020617] border border-slate-800 space-y-2 text-xs">
            <div className="flex items-center justify-between text-slate-300 font-semibold font-mono text-[11px]">
              <span className="flex items-center gap-1.5">
                <Sliders className="w-3.5 h-3.5 text-indigo-400" />
                AIQA Orchestration Heuristics
              </span>
              <span className="text-emerald-400">Dynamic Re-ordering: ACTIVE</span>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-2 text-[11px] font-mono text-slate-400">
              <div className="p-2 rounded bg-[#0B1120] border border-slate-800/80">
                <div className="text-slate-200 font-bold">1. Risk Score Filtering</div>
                <div>Prioritizes modified auth & transaction endpoints before secondary UI layers.</div>
              </div>
              <div className="p-2 rounded bg-[#0B1120] border border-slate-800/80">
                <div className="text-slate-200 font-bold">2. Flakiness Isolation</div>
                <div>Reroutes volatile telemetry tests to dedicated sandboxed workers.</div>
              </div>
              <div className="p-2 rounded bg-[#0B1120] border border-slate-800/80">
                <div className="text-slate-200 font-bold">3. Fast-Fail Gates</div>
                <div>Halts downstream DAG stages upon high-severity product security breaches.</div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
