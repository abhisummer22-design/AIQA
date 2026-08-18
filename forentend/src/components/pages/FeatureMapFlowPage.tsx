/**
 * AIQA Feature Map / Flow Page
 * Expose features and understandable project/system call flow.
 */

import React, { useState } from 'react';
import {
  Layers,
  Share2,
  Workflow,
  Activity,
  Play,
  CheckCircle2,
  AlertTriangle,
  Server,
  Database,
  Globe,
  Radio,
  ArrowRight,
  Sparkles,
  Search,
  Filter,
  Eye,
  RefreshCw,
} from 'lucide-react';
import { useAIQA } from '../../context/AIQAContext';
import { SectionHeader } from '../common/SectionHeader';
import { FeatureMapNode, SystemCallFlow } from '../../types';

export const FeatureMapFlowPage: React.FC = () => {
  const { addActivity } = useAIQA();

  const [featureNodes, setFeatureNodes] = useState<FeatureMapNode[]>([
    {
      id: 'NODE-AUTH-GW',
      name: 'Auth Gateway (OIDC/SAML)',
      module: 'AUTH',
      type: 'API_GATEWAY',
      healthScore: 99,
      testCoveragePct: 96,
      dependencies: ['NODE-TOKEN-SRV', 'NODE-USER-DB'],
      activeAlertsCount: 0,
    },
    {
      id: 'NODE-TOKEN-SRV',
      name: 'Token & Session Microservice',
      module: 'AUTH',
      type: 'MICROSERVICE',
      healthScore: 98,
      testCoveragePct: 94,
      dependencies: ['NODE-REDIS-CACHE'],
      activeAlertsCount: 0,
    },
    {
      id: 'NODE-PAY-GW',
      name: 'Payment Ingestion Engine',
      module: 'PAYMENTS',
      type: 'MICROSERVICE',
      healthScore: 95,
      testCoveragePct: 98,
      dependencies: ['NODE-LEDGER-SRV', 'NODE-STRIPE-PROXY'],
      activeAlertsCount: 0,
    },
    {
      id: 'NODE-LEDGER-SRV',
      name: 'Double-Entry Ledger Service',
      module: 'LEDGER',
      type: 'MICROSERVICE',
      healthScore: 97,
      testCoveragePct: 99,
      dependencies: ['NODE-SQL-DB', 'NODE-EVENT-BUS'],
      activeAlertsCount: 0,
    },
    {
      id: 'NODE-EVENT-BUS',
      name: 'Kafka / Event Notification Bus',
      module: 'NOTIFICATIONS',
      type: 'ASYNC_QUEUE',
      healthScore: 96,
      testCoveragePct: 91,
      dependencies: ['NODE-WEBHOOK-DISPATCH'],
      activeAlertsCount: 0,
    },
    {
      id: 'NODE-SQL-DB',
      name: 'Primary PostgreSQL Sharded Cluster',
      module: 'LEDGER',
      type: 'SQL_DATABASE',
      healthScore: 100,
      testCoveragePct: 97,
      dependencies: [],
      activeAlertsCount: 0,
    },
  ]);

  const [callFlows, setCallFlows] = useState<SystemCallFlow[]>([
    {
      id: 'FLOW-01',
      title: 'User Authentication & Multi-Tenant Session Exchange',
      entryTrigger: 'POST /api/v2/auth/oauth-token (Bearer exchange)',
      totalLatencyMs: 42,
      lastTracedAt: '2026-08-16 09:20 UTC',
      steps: [
        { stepNumber: 1, sourceNode: 'Edge Cloudflare CDN', targetNode: 'Auth Gateway (OIDC/SAML)', protocol: 'REST_HTTP', actionName: 'Validate TLS & Rate Limit Header', latencyMs: 8, status: 'HEALTHY' },
        { stepNumber: 2, sourceNode: 'Auth Gateway', targetNode: 'Token & Session Microservice', protocol: 'GRPC', actionName: 'Verify PKCE & Introspect Token', latencyMs: 14, status: 'HEALTHY' },
        { stepNumber: 3, sourceNode: 'Token Microservice', targetNode: 'Redis Session Cache', protocol: 'REST_HTTP', actionName: 'Cache RBAC Claims & TTL', latencyMs: 6, status: 'HEALTHY' },
        { stepNumber: 4, sourceNode: 'Token Microservice', targetNode: 'Event Notification Bus', protocol: 'EVENT_BUS', actionName: 'Publish user.logged_in Event', latencyMs: 14, status: 'HEALTHY' },
      ],
    },
    {
      id: 'FLOW-02',
      title: 'Checkout Payment & Double-Entry Ledger Reconciliation',
      entryTrigger: 'POST /api/v1/checkout/process-transaction',
      totalLatencyMs: 88,
      lastTracedAt: '2026-08-16 09:14 UTC',
      steps: [
        { stepNumber: 1, sourceNode: 'Client Web/Mobile App', targetNode: 'Payment Ingestion Engine', protocol: 'REST_HTTP', actionName: 'Submit Encrypted Payment Payload', latencyMs: 22, status: 'HEALTHY' },
        { stepNumber: 2, sourceNode: 'Payment Engine', targetNode: 'Stripe Vault Proxy', protocol: 'REST_HTTP', actionName: 'Execute 3DS Authorize Hold', latencyMs: 38, status: 'HEALTHY' },
        { stepNumber: 3, sourceNode: 'Payment Engine', targetNode: 'Double-Entry Ledger Service', protocol: 'GRPC', actionName: 'Atomically Record Debit/Credit Entries', latencyMs: 16, status: 'HEALTHY' },
        { stepNumber: 4, sourceNode: 'Ledger Service', targetNode: 'Primary PostgreSQL Sharded Cluster', protocol: 'DB_QUERY', actionName: 'Commit 2PC Distributed Transaction', latencyMs: 12, status: 'HEALTHY' },
      ],
    },
    {
      id: 'FLOW-03',
      title: 'Asynchronous Webhook & Event Dispatch Pipeline',
      entryTrigger: 'EventBus: transaction.settled -> Webhook Queue',
      totalLatencyMs: 35,
      lastTracedAt: '2026-08-16 09:05 UTC',
      steps: [
        { stepNumber: 1, sourceNode: 'Kafka / Event Notification Bus', targetNode: 'Webhook Dispatch Consumer', protocol: 'EVENT_BUS', actionName: 'Poll Partition Offset & Deserialize Event', latencyMs: 9, status: 'HEALTHY' },
        { stepNumber: 2, sourceNode: 'Webhook Consumer', targetNode: 'PostgreSQL Idempotency Store', protocol: 'DB_QUERY', actionName: 'Check Message Deduplication Key', latencyMs: 11, status: 'HEALTHY' },
        { stepNumber: 3, sourceNode: 'Webhook Consumer', targetNode: 'Partner External Webhook API', protocol: 'REST_HTTP', actionName: 'Dispatch Signed HMAC Payload', latencyMs: 15, status: 'HEALTHY' },
      ],
    },
  ]);

  const [selectedFlowId, setSelectedFlowId] = useState<string>('FLOW-02');
  const [isSimulatingTrace, setIsSimulatingTrace] = useState(false);
  const [activeStepIndex, setActiveStepIndex] = useState<number>(-1);

  const selectedFlow = callFlows.find((f) => f.id === selectedFlowId) || callFlows[0];

  const handleSimulateFlow = () => {
    setIsSimulatingTrace(true);
    setActiveStepIndex(0);

    const timer1 = setTimeout(() => setActiveStepIndex(1), 500);
    const timer2 = setTimeout(() => setActiveStepIndex(2), 1000);
    const timer3 = setTimeout(() => setActiveStepIndex(3), 1500);
    const timer4 = setTimeout(() => {
      setActiveStepIndex(-1);
      setIsSimulatingTrace(false);
      addActivity({
        type: 'TEST_COMPLETED',
        title: `Flow Trace Simulated: ${selectedFlow.id}`,
        description: `Verified all ${selectedFlow.steps.length} hops with total latency of ${selectedFlow.totalLatencyMs}ms.`,
        actor: 'Feature Flow Tracer',
      });
    }, 2100);
  };

  return (
    <div className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="Feature Map / System Call Flow"
        subtitle="Expose architectural features, microservice topology, dependencies, and understandable project call flow traces"
        actions={
          <div className="flex items-center gap-2">
            <button
              onClick={handleSimulateFlow}
              disabled={isSimulatingTrace}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-indigo-600 hover:bg-indigo-500 text-white font-semibold text-xs shadow-sm shadow-indigo-950/60 cursor-pointer disabled:opacity-50"
            >
              <Play className="w-3.5 h-3.5" />
              <span>{isSimulatingTrace ? 'Tracing Step Hops...' : 'Simulate Call Flow'}</span>
            </button>
          </div>
        }
      />

      {/* Feature Map Top Metrics */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-2.5">
        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Mapped Feature Nodes</span>
            <Layers className="w-3.5 h-3.5 text-indigo-400" />
          </div>
          <div className="text-lg font-bold font-mono text-slate-100">
            {featureNodes.length} Microservices & Gateways
          </div>
          <div className="text-[10px] text-slate-500">100% Topologically mapped</div>
        </div>

        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>Mean Feature Test Coverage</span>
            <CheckCircle2 className="w-3.5 h-3.5 text-emerald-400" />
          </div>
          <div className="text-lg font-bold font-mono text-emerald-400">
            {Math.round(
              featureNodes.reduce((acc, f) => acc + f.testCoveragePct, 0) / featureNodes.length
            )}
            % Coverage
          </div>
          <div className="text-[10px] text-slate-500">Contract & E2E assertions</div>
        </div>

        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>End-to-End Call Flows</span>
            <Workflow className="w-3.5 h-3.5 text-cyan-400" />
          </div>
          <div className="text-lg font-bold font-mono text-cyan-300">
            {callFlows.length} Documented Traces
          </div>
          <div className="text-[10px] text-slate-500">Understandable call graph</div>
        </div>

        <div className="p-3 rounded-lg border border-slate-800 bg-[#0B1120] space-y-1">
          <div className="flex items-center justify-between text-slate-400 font-mono text-[10px]">
            <span>System Health Index</span>
            <Activity className="w-3.5 h-3.5 text-emerald-400" />
          </div>
          <div className="text-lg font-bold font-mono text-emerald-400">99.2% Nominal</div>
          <div className="text-[10px] text-slate-500">0 critical bottleneck points</div>
        </div>
      </div>

      {/* Feature Map Visual Grid */}
      <div className="p-4 rounded-lg border border-slate-800 bg-[#0B1120] space-y-3">
        <div className="flex items-center justify-between pb-2 border-b border-slate-800">
          <span className="text-xs font-bold text-slate-200 uppercase font-mono">
            System Feature & Microservice Topology
          </span>
          <span className="text-[10px] font-mono text-slate-400">
            Live Dependency Graph &amp; Coverage
          </span>
        </div>

        <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-2.5">
          {featureNodes.map((node) => (
            <div
              key={node.id}
              className="p-3 rounded-md bg-[#020617] border border-slate-800 space-y-2 text-xs"
            >
              <div className="flex items-center justify-between">
                <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-indigo-950 text-indigo-300 border border-indigo-800 font-bold">
                  {node.module}
                </span>
                <span className="text-[10px] font-mono font-bold text-emerald-400">
                  {node.healthScore}%
                </span>
              </div>
              <div className="font-semibold text-slate-100 text-xs line-clamp-2">
                {node.name}
              </div>
              <div className="pt-1 border-t border-slate-800/80 flex items-center justify-between text-[10px] font-mono text-slate-400">
                <span>Coverage:</span>
                <strong className="text-slate-200">{node.testCoveragePct}%</strong>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* Main Grid: Call Flow Traces & Interactive Step Inspection */}
      <div className="grid grid-cols-1 lg:grid-cols-12 gap-4">
        {/* Left Column: Call Flow Selector (4 cols) */}
        <div className="lg:col-span-4 rounded-lg border border-slate-800 bg-[#0B1120] p-3 space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-slate-800">
            <span className="text-xs font-bold text-slate-200 uppercase font-mono">
              System Call Flows ({callFlows.length})
            </span>
          </div>

          <div className="space-y-2">
            {callFlows.map((flow) => {
              const isSelected = flow.id === selectedFlowId;
              return (
                <div
                  key={flow.id}
                  onClick={() => setSelectedFlowId(flow.id)}
                  className={`p-2.5 rounded-md border text-xs cursor-pointer transition-all ${
                    isSelected
                      ? 'border-indigo-500 bg-indigo-950/40 text-slate-100 shadow-sm'
                      : 'border-slate-800 bg-[#020617] hover:border-slate-700 text-slate-300'
                  }`}
                >
                  <div className="flex items-center justify-between gap-1 mb-1">
                    <span className="font-mono text-xs font-bold text-indigo-400">{flow.id}</span>
                    <span className="text-[10px] font-mono text-emerald-400 font-bold">
                      {flow.totalLatencyMs}ms
                    </span>
                  </div>
                  <h4 className="font-semibold text-slate-100 text-xs mb-1">{flow.title}</h4>
                  <div className="text-[10px] font-mono text-slate-400 truncate">
                    {flow.entryTrigger}
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Right Column: Step-by-Step Flow Visualizer (8 cols) */}
        <div className="lg:col-span-8 rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-4 text-xs">
          <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 pb-3 border-b border-slate-800">
            <div>
              <div className="flex items-center gap-2">
                <span className="font-mono font-bold text-indigo-400">{selectedFlow.id}</span>
                <h3 className="text-sm font-bold text-slate-100">{selectedFlow.title}</h3>
              </div>
              <p className="text-xs text-slate-400 font-mono mt-0.5">
                Entrypoint: <strong className="text-slate-300">{selectedFlow.entryTrigger}</strong>
              </p>
            </div>
            <div className="text-right font-mono text-xs">
              <span className="text-slate-400">Total Latency:</span>{' '}
              <strong className="text-indigo-400">{selectedFlow.totalLatencyMs}ms</strong>
            </div>
          </div>

          {/* Sequential Step Hops */}
          <div className="space-y-2.5">
            <div className="text-[11px] font-mono uppercase text-slate-400 font-bold">
              Sequential Microservice Call Graph:
            </div>

            <div className="space-y-2">
              {selectedFlow.steps.map((step, idx) => {
                const isActiveHop = activeStepIndex === idx;
                return (
                  <div
                    key={step.stepNumber}
                    className={`p-3 rounded-lg border transition-all ${
                      isActiveHop
                        ? 'border-indigo-400 bg-indigo-950/60 shadow-md shadow-indigo-950'
                        : 'border-slate-800 bg-[#020617]'
                    }`}
                  >
                    <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2 text-xs">
                      <div className="flex items-center gap-2.5">
                        <div
                          className={`w-6 h-6 rounded flex items-center justify-center font-mono font-bold text-[11px] shrink-0 ${
                            isActiveHop
                              ? 'bg-indigo-600 text-white animate-bounce'
                              : 'bg-[#0B1120] text-indigo-400 border border-slate-700'
                          }`}
                        >
                          {step.stepNumber}
                        </div>
                        <div className="space-y-0.5">
                          <div className="font-semibold text-slate-200 flex items-center gap-2">
                            <span>{step.sourceNode}</span>
                            <ArrowRight className="w-3.5 h-3.5 text-indigo-400 shrink-0" />
                            <span className="text-indigo-300">{step.targetNode}</span>
                          </div>
                          <div className="text-[10px] font-mono text-slate-400">
                            Action: <span className="text-slate-200">{step.actionName}</span>
                          </div>
                        </div>
                      </div>

                      <div className="flex items-center gap-3 font-mono text-[11px]">
                        <span className="px-1.5 py-0.2 rounded bg-slate-800 text-slate-300 border border-slate-700 text-[9px] font-bold">
                          {step.protocol}
                        </span>
                        <span className="text-slate-400">{step.latencyMs}ms</span>
                        <span className="text-[9px] font-mono px-1.5 py-0.2 rounded bg-emerald-950 text-emerald-300 border border-emerald-800 font-bold">
                          {step.status}
                        </span>
                      </div>
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
