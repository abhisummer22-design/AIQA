/**
 * Neural Pipeline — the real AIQA execution loop, live.
 *
 * This is the agent-loop diagram people expect from a system like this
 * (router → plan → tools → execute → observe → goal, with memory alongside),
 * but every node is backed by a real module and a real number from
 * `GET /api/v1/projects/:id/pipeline`, and it animates from real events, not
 * on a timer.
 *
 * The part that matters most here is what it refuses to draw. A pipeline
 * picture is exactly the kind of UI that invites fabrication — it looks
 * complete, so the eye assumes every box works. A stage the real backend
 * reports as `available: false` is therefore rendered explicitly as
 * unavailable, with the real reason from the backend, in its own "Not part
 * of this pipeline" section below — never silently omitted (which would
 * imply the loop is closed) and never animated like the working ones
 * (which would be an outright lie).
 *
 * Not every real, working stage belongs in the strict sequential loop
 * above, though (Re-planning/Vector Store/RAG Retrieval genuinely exist as
 * of this feature, but are not steps a run passes through in order the way
 * router → plan → execute → observe → goal is) — those render in their own
 * "Supporting capabilities" section instead, derived from the real
 * response (`available && not in LOOP_ORDER && not memory`), not a second
 * hardcoded id list to keep in sync by hand.
 *
 * "Live" here is real: the flow animation runs only while this project
 * genuinely has tasks executing, and the component refetches on real
 * `RealtimeContext` events rather than polling on a clock.
 */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import {
  AlertTriangle,
  Boxes,
  BrainCircuit,
  CheckCircle2,
  Cpu,
  Eye,
  GitBranch,
  Loader2,
  Network,
  Repeat,
  Search,
  Target,
} from 'lucide-react';
import { aiqaApi, ApiError } from '../../services/aiqaApi';
import { useRealtime } from '../../context/RealtimeContext';
import type { NeuralPipelineResponse, PipelineStage } from '../../types/contract';

const STAGE_ICONS: Record<string, React.ComponentType<{ className?: string }>> = {
  brain: BrainCircuit,
  planning: GitBranch,
  tools: Cpu,
  execution: Boxes,
  observation: Eye,
  memory: Network,
  goal: Target,
  replanning: Repeat,
  vector: Search,
  rag: Search,
};

/** The real loop order, matching how a run actually moves through the
 *  backend: router picks a model, the plan is validated into a DAG, tasks are
 *  routed to executors, executed, observed via events, and the run reaches a
 *  terminal state. */
const LOOP_ORDER = ['brain', 'planning', 'tools', 'execution', 'observation', 'goal'];

const StageNode: React.FC<{
  stage: PipelineStage;
  active: boolean;
  index: number;
}> = ({ stage, active, index }) => {
  const Icon = STAGE_ICONS[stage.id] ?? Boxes;
  const unavailable = !stage.available;

  return (
    <div
      id={`pipeline-stage-${stage.id}`}
      title={stage.backing}
      className={`relative rounded-lg border p-2.5 transition-colors ${
        unavailable
          ? 'border-slate-800 bg-[#020617]/60 border-dashed'
          : active
            ? 'border-yellow-600/80 bg-yellow-950/30 shadow-[0_0_18px_-4px] shadow-yellow-500/50'
            : 'border-slate-800 bg-[#020617]'
      }`}
      style={active ? { animation: `pipelinePulse 1.6s ease-in-out ${index * 0.12}s infinite` } : undefined}
    >
      <div className="flex items-center gap-2 min-w-0">
        <Icon
          className={`w-4 h-4 shrink-0 ${
            unavailable ? 'text-slate-600' : active ? 'text-yellow-300' : 'text-violet-400'
          }`}
        />
        <span
          className={`text-[11px] font-mono font-bold truncate ${
            unavailable ? 'text-slate-500 line-through' : 'text-slate-100'
          }`}
        >
          {stage.label}
        </span>
      </div>

      {stage.metric !== null && stage.metric_label && (
        <div className="mt-1 text-[10px] font-mono text-slate-400">
          <span className={active ? 'text-yellow-300 font-bold' : 'text-slate-300 font-bold'}>{stage.metric}</span>{' '}
          {stage.metric_label}
        </div>
      )}

      {unavailable && (
        <div className="mt-1 text-[9px] font-mono text-amber-400/90 uppercase tracking-wide">not implemented</div>
      )}
    </div>
  );
};

export interface NeuralPipelineProps {
  projectId: string | null;
}

export const NeuralPipeline: React.FC<NeuralPipelineProps> = ({ projectId }) => {
  const realtime = useRealtime();
  const [data, setData] = useState<NeuralPipelineResponse | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const [loading, setLoading] = useState(false);
  const isUnmounted = useRef(false);

  const load = useCallback(async (pid: string, signal?: AbortSignal) => {
    setLoading(true);
    try {
      const res = await aiqaApi.pipeline.get(pid, signal);
      if (signal?.aborted || isUnmounted.current) return;
      // Structural guard — an unexpected payload must not crash the page
      // that hosts this panel.
      if (!res || !Array.isArray(res.stages)) {
        setData(null);
        setError(null);
        return;
      }
      setData(res);
      setError(null);
    } catch (err) {
      if (signal?.aborted || isUnmounted.current) return;
      setData(null);
      if (err instanceof ApiError) setError(err);
    } finally {
      if (!signal?.aborted && !isUnmounted.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    isUnmounted.current = false;
    if (!projectId) {
      setData(null);
      setError(null);
      return;
    }
    const controller = new AbortController();
    void load(projectId, controller.signal);
    return () => {
      isUnmounted.current = true;
      controller.abort();
    };
  }, [projectId, load]);

  // Real events drive the refresh — no clock polling.
  useEffect(() => {
    if (!projectId) return;
    return realtime.subscribe(() => {
      void load(projectId);
    });
  }, [projectId, realtime, load]);

  const stageById = (id: string) => data?.stages.find((s) => s.id === id) ?? null;
  const execution = stageById('execution');
  // "Live" is a real fact: tasks are genuinely executing right now.
  const isLive = (execution?.metric ?? 0) > 0;
  const unavailableStages = data?.stages.filter((s) => !s.available) ?? [];
  // Real, working capabilities that are not a step of the strict
  // sequential loop above (Re-planning/Vector Store/RAG Retrieval) —
  // derived from the real response, not a second hardcoded id list, so a
  // future real stage of this same "supporting, not sequential" shape
  // shows up here automatically rather than silently vanishing the way
  // these three would have before this derivation existed.
  const supportingStages = data?.stages.filter((s) => s.available && !LOOP_ORDER.includes(s.id) && s.id !== 'memory') ?? [];

  return (
    <div id="neural-pipeline-panel" className="rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-3">
      <style>{`
        @keyframes pipelinePulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.55; }
        }
        @keyframes pipelineFlow {
          0% { background-position: 0 0; }
          100% { background-position: 24px 0; }
        }
      `}</style>

      <div className="flex items-center justify-between gap-2 pb-2 border-b border-slate-800/80">
        <div className="flex items-center gap-2 min-w-0">
          <BrainCircuit className="w-4 h-4 text-yellow-400 shrink-0" />
          <div className="min-w-0">
            <h2 className="text-sm font-bold text-slate-100 tracking-tight">Neural Pipeline</h2>
            <p className="text-[11px] text-slate-500">Real execution loop — each stage backed by a real module</p>
          </div>
        </div>
        <div className="flex items-center gap-1.5 shrink-0">
          {loading && <Loader2 className="w-3.5 h-3.5 animate-spin text-violet-400" />}
          <span
            className={`inline-flex items-center gap-1 px-2 py-0.5 rounded border text-[10px] font-mono font-bold ${
              isLive
                ? 'text-yellow-300 bg-yellow-950/60 border-yellow-800'
                : 'text-slate-400 bg-slate-800/60 border-slate-700'
            }`}
          >
            {isLive && <span className="w-1.5 h-1.5 rounded-full bg-yellow-400 animate-ping" />}
            {isLive ? 'EXECUTING' : 'IDLE'}
          </span>
        </div>
      </div>

      {!projectId ? (
        <p className="text-[11px] text-slate-400">Select a project to load its real pipeline state.</p>
      ) : error ? (
        <div className="flex items-start gap-2">
          <AlertTriangle className="w-3.5 h-3.5 text-rose-400 shrink-0 mt-0.5" />
          <div>
            <p className="text-[11px] text-slate-400">{error.message}</p>
            <button
              type="button"
              onClick={() => void load(projectId)}
              className="mt-1 text-[10px] font-mono text-violet-400 hover:text-violet-300 cursor-pointer"
            >
              Retry
            </button>
          </div>
        </div>
      ) : !data ? (
        <div className="flex items-center gap-2 text-xs font-mono text-slate-400 py-2">
          <Loader2 className="w-3.5 h-3.5 animate-spin text-violet-400" /> Loading real pipeline…
        </div>
      ) : (
        <>
          {/* The real loop, in the order a run actually travels. */}
          <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-6 gap-2">
            {LOOP_ORDER.map((id, i) => {
              const stage = stageById(id);
              if (!stage) return null;
              return <StageNode key={id} stage={stage} active={isLive} index={i} />;
            })}
          </div>

          {/* Flow rail — animated only while execution is genuinely live, so
              motion always means something real is happening. */}
          <div
            className="h-0.5 rounded-full"
            style={{
              backgroundImage: isLive
                ? 'repeating-linear-gradient(90deg, rgb(250 204 21) 0 8px, transparent 8px 24px)'
                : 'repeating-linear-gradient(90deg, rgb(51 65 85) 0 8px, transparent 8px 24px)',
              backgroundSize: '24px 100%',
              animation: isLive ? 'pipelineFlow 0.8s linear infinite' : undefined,
            }}
          />

          {/* Persistent memory — the only real one this system has. */}
          {(() => {
            const memory = stageById('memory');
            if (!memory) return null;
            return (
              <div className="flex items-center justify-between gap-2 px-2.5 py-2 rounded-md bg-[#020617] border border-slate-800">
                <div className="flex items-center gap-2 min-w-0">
                  <Network className="w-3.5 h-3.5 text-violet-400 shrink-0" />
                  <span className="text-[11px] font-mono text-slate-200">Quality Graph</span>
                </div>
                <div className="text-[10px] font-mono text-slate-400 shrink-0">
                  <span className="text-slate-200 font-bold">{data.graph_nodes}</span> nodes ·{' '}
                  <span className="text-slate-200 font-bold">{data.graph_relations}</span> relations
                </div>
              </div>
            );
          })()}

          {/* Real, working capabilities alongside the loop, not a step of
              it — same treatment "Persistent memory" already gets, just
              derived rather than a single hardcoded id. */}
          {supportingStages.length > 0 && (
            <div className="space-y-1.5 pt-1">
              <p className="text-[10px] font-mono uppercase tracking-wider text-slate-500">
                Supporting capabilities
              </p>
              <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
                {supportingStages.map((s, i) => (
                  <StageNode key={s.id} stage={s} active={false} index={i} />
                ))}
              </div>
            </div>
          )}

          {/* What this pipeline genuinely does NOT have. */}
          {unavailableStages.length > 0 && (
            <div className="space-y-1.5 pt-1">
              <p className="text-[10px] font-mono uppercase tracking-wider text-slate-500">
                Not part of this pipeline
              </p>
              <div className="grid grid-cols-1 sm:grid-cols-3 gap-2">
                {unavailableStages.map((s, i) => (
                  <StageNode key={s.id} stage={s} active={false} index={i} />
                ))}
              </div>
              <ul className="space-y-1">
                {unavailableStages.map((s) => (
                  <li key={s.id} className="text-[10px] font-mono text-slate-500 leading-snug">
                    <span className="text-slate-400">{s.label}:</span> {s.backing}
                  </li>
                ))}
              </ul>
            </div>
          )}

          <p className="text-[10px] font-mono text-slate-500 flex items-start gap-1.5">
            <CheckCircle2 className="w-3 h-3 shrink-0 mt-0.5" />
            <span>
              Flow animates only while tasks are genuinely executing. {data.active_runs} active run
              {data.active_runs === 1 ? '' : 's'} · {data.completed_runs} completed.
            </span>
          </p>
        </>
      )}
    </div>
  );
};
