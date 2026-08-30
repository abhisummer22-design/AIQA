/**
 * API Testing Flow rail — a real-time, top-to-bottom read of the one real
 * HTTP request this page's own form is building, fed from
 * `APITestingPage`'s own state (`method`, `status`, `result`) — no second
 * fetch. Unlike Performance Testing's repeated-sample loop, this page
 * executes exactly one real request per click, so "real-time" here means
 * the three real stages that one request actually passes through: built,
 * dispatched (the real agent — `executor::CompiledApiExecutor`, a direct
 * `reqwest` call, no simulated timing), and evaluated against real
 * assertions.
 *
 * As with `PerformanceFlowRail`/`OrchestratorRadar`, every detail states a
 * fact the panels beside it don't already spell out verbatim.
 */
import React from 'react';
import { ListChecks, Send, Zap } from 'lucide-react';
import { FlowConnector, FlowNode, type FlowNodeState } from '../common/FlowRail';
import type { ExecStatus } from '../pages/APITestingPage';
import type { ApiTestExecutionResponse } from '../../types/contract';

export interface ApiTestingFlowRailProps {
  method: string;
  status: ExecStatus;
  result: ApiTestExecutionResponse | null;
}

export const ApiTestingFlowRail: React.FC<ApiTestingFlowRailProps> = ({ method, status, result }) => {
  const isExecuting = status === 'EXECUTING';
  const isFailure = status === 'FAILED' || status === 'INFRA_ERROR' || status === 'REQUEST_FAILED' || status === 'PROJECT_ACCESS_DENIED' || status === 'VALIDATION_FAILED';
  const hasRun = status === 'PASSED' || status === 'FAILED' || status === 'STILL_RUNNING';

  const executeState: FlowNodeState = isExecuting ? 'active' : status === 'PASSED' ? 'done' : isFailure ? 'failed' : 'idle';
  const executeDetail = isExecuting
    ? 'Live request in flight'
    : status === 'PASSED' && result?.result
      ? `${result.result.request_duration_ms} ms response`
      : isFailure
        ? 'Not completed — see details below'
        : status === 'STILL_RUNNING'
          ? 'Past wait ceiling, still running'
          : 'Not executed yet';

  const assertionResults = result?.result?.assertions ?? [];
  const passCount = assertionResults.filter((a) => a.passed).length;
  const assertionsState: FlowNodeState =
    !hasRun || assertionResults.length === 0 ? 'idle' : passCount === assertionResults.length ? 'done' : 'failed';
  const assertionsDetail =
    !hasRun ? 'Awaiting execution' : assertionResults.length === 0 ? 'None configured' : `${passCount}/${assertionResults.length} assertions passed`;

  return (
    <div id="api-testing-flow-rail" className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 h-full flex flex-col">
      {/* The futuristic glowing badge sits to the left of the title — a
          real state indicator, not decoration: it brightens and pulses
          only while the real agent (CompiledApiExecutor) is genuinely
          mid-request. */}
      <div className="flex items-center gap-2 mb-3">
        <div
          className={`relative w-8 h-8 rounded-full border flex items-center justify-center shrink-0 transition-all ${
            isExecuting
              ? 'border-cyan-400 bg-cyan-950/60 shadow-[0_0_14px_2px_rgba(34,211,238,0.55)]'
              : hasRun
                ? 'border-cyan-700 bg-cyan-950/30 shadow-[0_0_8px_0px_rgba(34,211,238,0.25)]'
                : 'border-slate-700 bg-slate-900'
          }`}
        >
          {isExecuting && <span className="absolute inset-0 rounded-full border border-cyan-400 animate-ping" />}
          <Send className={`w-3.5 h-3.5 relative ${isExecuting ? 'text-cyan-300' : hasRun ? 'text-cyan-500' : 'text-slate-500'}`} />
        </div>
        <h3 className="text-[10px] font-bold text-slate-100 font-mono uppercase tracking-wider">API Agent</h3>
      </div>

      <FlowNode icon={Zap} label="Request" state="idle" detail={`${method} request built`} />
      <FlowConnector animated={isExecuting} />
      <FlowNode icon={Send} label="Execute" state={executeState} detail={executeDetail} />
      <FlowConnector animated={isExecuting} />
      <FlowNode icon={ListChecks} label="Assertions" state={assertionsState} detail={assertionsDetail} />
    </div>
  );
};
