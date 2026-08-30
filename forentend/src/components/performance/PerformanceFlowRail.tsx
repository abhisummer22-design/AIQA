/**
 * Performance Flow rail — a real-time, top-to-bottom read of the same
 * workload this page's own form and live-metrics panel are driving, fed
 * from `PerformanceTestingPage`'s own state (`testType`, `method`, `live`)
 * — no second fetch. There is no AI/LLM agent in this pipeline (performance
 * testing reuses the real HTTP execution core, `executor::execute_plan_once`
 * — see this page's own module doc comment), so "the agent" here is that
 * real request-dispatch step, not a model.
 *
 * Each node states a fact the detailed panels beside it don't already spell
 * out verbatim (a phrase, not a bare repeat of a stat cell), so this rail
 * adds real information rather than echoing `LiveMetricsPanel`.
 */
import React from 'react';
import { Globe, ShieldCheck, Zap } from 'lucide-react';
import { FlowConnector, FlowNode, type FlowNodeState } from '../common/FlowRail';
import type { PerformanceRunStatusResponse } from '../../types/contract';

export interface PerformanceFlowRailProps {
  testType: string;
  method: string;
  live: PerformanceRunStatusResponse | null;
}

export const PerformanceFlowRail: React.FC<PerformanceFlowRailProps> = ({ testType, method, live }) => {
  const isRunning = live?.status === 'RUNNING';
  const isError = live?.status === 'ERROR' || live?.status === 'INTERRUPTED';
  const hasSamples = (live?.sample_count ?? 0) > 0;

  const dispatchState: FlowNodeState = isRunning ? 'active' : isError ? 'failed' : hasSamples ? 'done' : 'idle';
  // On a genuine execution error, the real reason is disclosed exactly
  // once, in the dedicated Execution Error panel below — this node states
  // only that dispatch stopped, never a second copy of that same reason.
  const dispatchDetail = isRunning
    ? 'Live requests in flight'
    : isError
      ? 'Stopped — see Execution Error'
      : hasSamples && live?.metrics
        ? `${live.metrics.total_requests} request${live.metrics.total_requests === 1 ? '' : 's'} completed`
        : 'Not started';

  const verdictState: FlowNodeState =
    live?.verdict === 'PASSED' ? 'done' : live?.verdict === 'FAILED' || isError ? 'failed' : 'idle';
  const verdictDetail = !live
    ? 'Awaiting run'
    : isRunning
      ? 'Pending completion'
      : live.verdict === 'PASSED'
        ? 'Thresholds passed'
        : live.verdict === 'FAILED'
          ? 'Thresholds failed'
          : live.status === 'CANCELLED'
            ? 'Run stopped early'
            : isError
              ? 'No verdict — execution failed'
              : 'Completed, no thresholds set';

  return (
    <div id="performance-flow-rail" className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 h-full flex flex-col">
      <h3 className="text-[10px] font-bold text-slate-100 font-mono uppercase tracking-wider mb-3">Performance Flow</h3>

      <FlowNode icon={Globe} label="Workload" state="idle" detail={`${testType} · ${method}`} />
      <FlowConnector animated={isRunning} />
      <FlowNode icon={Zap} label="Dispatch" state={dispatchState} detail={dispatchDetail} />
      <FlowConnector animated={isRunning} />
      <FlowNode icon={ShieldCheck} label="Verdict" state={verdictState} detail={verdictDetail} />
    </div>
  );
};
