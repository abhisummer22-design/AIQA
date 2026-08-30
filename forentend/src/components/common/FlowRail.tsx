/**
 * Shared visual primitives for a narrow, top-to-bottom "real flow" rail —
 * a stack of stage nodes joined by a connecting line, used wherever a page
 * shows its own real, live pipeline state in a side column (first built for
 * `TestGenerationFlow`, reused by `OrchestratorFlowRail`).
 *
 * These two components are pure presentation: every state/label a caller
 * passes must already be a real value from that page's own data. Nothing
 * here fetches, guesses, or animates on a timer — `FlowConnector`'s
 * `animated` flag is the caller's own honest claim that something between
 * these two stages is genuinely in progress right now.
 */
import React from 'react';
import { Loader2, XCircle } from 'lucide-react';

export type FlowNodeState = 'idle' | 'active' | 'done' | 'failed';

const nodeDot: Record<FlowNodeState, string> = {
  idle: 'bg-slate-700 border-slate-600',
  active: 'bg-indigo-500 border-indigo-400 animate-pulse',
  done: 'bg-emerald-500 border-emerald-400',
  failed: 'bg-rose-500 border-rose-400',
};

const nodeText: Record<FlowNodeState, string> = {
  idle: 'text-slate-500',
  active: 'text-indigo-300',
  done: 'text-emerald-300',
  failed: 'text-rose-300',
};

export const FlowConnector: React.FC<{ animated: boolean }> = ({ animated }) => (
  <div className="flex justify-start pl-[9px]">
    <div className={`w-px h-4 ${animated ? 'bg-gradient-to-b from-indigo-500 to-indigo-500/10 animate-pulse' : 'bg-slate-700'}`} />
  </div>
);

export const FlowNode: React.FC<{
  icon: React.ComponentType<{ className?: string }>;
  label: string;
  state: FlowNodeState;
  detail: string;
}> = ({ icon: Icon, label, state, detail }) => (
  <div className="flex items-start gap-2">
    <div className={`mt-0.5 w-[19px] h-[19px] rounded-full border flex items-center justify-center shrink-0 ${nodeDot[state]}`}>
      {state === 'active' ? (
        <Loader2 className="w-2.5 h-2.5 text-white animate-spin" />
      ) : state === 'failed' ? (
        <XCircle className="w-2.5 h-2.5 text-white" />
      ) : (
        <Icon className="w-2.5 h-2.5 text-white" />
      )}
    </div>
    <div className="min-w-0 flex-1">
      <p className={`text-[10px] font-mono font-bold uppercase tracking-wider ${nodeText[state]}`}>{label}</p>
      <p className="text-[10px] text-slate-500 leading-snug break-words">{detail}</p>
    </div>
  </div>
);
