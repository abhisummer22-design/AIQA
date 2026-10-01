/**
 * AIQA Orchestrator — the real, realtime orchestration control plane.
 *
 * History: this page previously rendered a fully fabricated orchestration UI
 * (hardcoded `PLAN-901`/`PLAN-902` plans, an invented DAG, fake "specialized
 * agent" worker pools, a risk/change/test-value scoring model with no
 * computation behind it). That was replaced by an honest "deferred" empty
 * state, because none of it existed in the backend.
 *
 * This version replaces that empty state with the orchestration that *does*
 * genuinely exist and is genuinely running. Every value rendered here comes
 * from a real backend response — nothing is simulated, and nothing claims a
 * capability that was not actually measured:
 *
 *   - **AI provider routing** (`GET /api/v1/ai/status`) — the real
 *     `routing_preview` produced by `ai_router.rs::score_model`: which
 *     provider/model would win right now, its real heuristic score, the real
 *     stated reason, the real AUTO/MANUAL mode, and real per-provider health.
 *     The score is a documented routing heuristic, NOT an accuracy or quality
 *     benchmark — labelled as such here, matching that module's own doc
 *     comment.
 *   - **Executor capability routing + policy gates**
 *     (`GET /api/v1/orchestrator/state`) — the real `ExecutorRegistry`
 *     registered kinds a task can be routed to, each with the real gate
 *     `policy.rs::evaluate` will apply to it.
 *   - **Live DAG** (same endpoint) — real task states for every run this
 *     backend process is currently driving, including real prerequisite
 *     edges. Honest limitation, stated in the UI: only in-process runs
 *     appear; finished runs from an earlier process are history and live in
 *     the Execution Hub.
 *   - **Live activity stream** — the existing, already-real
 *     `OrchestratorLogsPanel` (WebSocket `/api/v1/logs/ws`), reused rather
 *     than reimplemented. Routing decisions now emit on their own
 *     `ai_router` source (`api.rs`), so a real routing choice — including a
 *     truthful fallback — is visible here as it happens.
 *
 * What is still genuinely NOT implemented — and is still disclosed as such
 * below rather than quietly dropped — is AI-assisted *autonomous* routing:
 * an AI deciding which specialized QA agent should own which task. No
 * orchestration-plan entity and no specialized-agent worker pool exists in
 * the backend (ACC-006, deferred by product-owner decision). The routing
 * shown here is real capability/score routing, which is a different thing,
 * and is not relabelled as the deferred one.
 */

import React, { useCallback, useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import {
  AlertTriangle,
  CheckCircle2,
  CircleSlash,
  Cpu,
  GitBranch,
  Info,
  Loader2,
  Network,
  PlayCircle,
  RefreshCw,
  ShieldAlert,
  Workflow,
} from 'lucide-react';
import { SectionHeader } from '../common/SectionHeader';
import { OrchestratorLogsPanel } from '../dashboard/OrchestratorLogsPanel';
import { NeuralPipeline } from '../orchestrator/NeuralPipeline';
import { OrchestratorRadar } from '../orchestrator/OrchestratorRadar';
import { aiqaApi, ApiError } from '../../services/aiqaApi';
import { useAuth } from '../../context/AuthContext';
import { useProject } from '../../context/ProjectContext';
import { useRealtime } from '../../context/RealtimeContext';
import { renderAuthState, renderResourceErrorState } from '../state/StateViews';
import type { AiStatusResponse, OrchestratorStateResponse, TaskState } from '../../types/contract';

/** Safety-net refresh only. The DAG is driven by real pushed events (see
 *  `useRealtime().subscribe` below) — this interval exists solely for AI
 *  provider health, which genuinely has no event stream on the backend:
 *  `ai_router.rs` re-discovers lazily on a TTL, it never pushes. Deliberately
 *  slow, and labelled as polled in the UI, so nothing here claims a push
 *  feed that does not exist. */
const HEALTH_POLL_MS = 30000;

const gateStyles: Record<string, string> = {
  AUTO: 'text-emerald-400 bg-emerald-950/60 border-emerald-800',
  REQUIRES_APPROVAL: 'text-amber-400 bg-amber-950/60 border-amber-800',
  BLOCKED: 'text-rose-400 bg-rose-950/60 border-rose-800',
};

const providerStatusStyles: Record<string, string> = {
  HEALTHY: 'text-emerald-400 bg-emerald-950/60 border-emerald-800',
  AVAILABLE: 'text-emerald-400 bg-emerald-950/60 border-emerald-800',
  DEGRADED: 'text-amber-400 bg-amber-950/60 border-amber-800',
  NOT_CONFIGURED: 'text-slate-400 bg-slate-800/60 border-slate-700',
  UNAVAILABLE: 'text-rose-400 bg-rose-950/60 border-rose-800',
  ERROR: 'text-rose-400 bg-rose-950/60 border-rose-800',
};

const taskStatusStyles: Record<string, string> = {
  PASSED: 'text-emerald-400 bg-emerald-950/60 border-emerald-800',
  RUNNING: 'text-violet-300 bg-violet-950/60 border-violet-800',
  QUEUED: 'text-slate-400 bg-slate-800/60 border-slate-700',
  BLOCKED: 'text-amber-400 bg-amber-950/60 border-amber-800',
  FAILED: 'text-rose-400 bg-rose-950/60 border-rose-800',
  TIMEOUT: 'text-rose-400 bg-rose-950/60 border-rose-800',
  INFRA_ERROR: 'text-orange-400 bg-orange-950/60 border-orange-800',
  SKIPPED: 'text-content-muted bg-slate-900 border-slate-800',
  CANCELLED: 'text-content-muted bg-slate-900 border-slate-800',
  NEEDS_REVIEW: 'text-amber-400 bg-amber-950/60 border-amber-800',
};

const Pill: React.FC<{ label: string; className?: string }> = ({ label, className }) => (
  <span
    className={`inline-flex items-center px-1.5 py-0.5 rounded border text-[9px] font-mono font-bold uppercase tracking-wide ${
      className ?? 'text-slate-400 bg-slate-800/60 border-slate-700'
    }`}
  >
    {label}
  </span>
);

const PanelCard: React.FC<{
  icon: React.ComponentType<{ className?: string }>;
  iconClassName: string;
  title: string;
  subtitle: string;
  headerRight?: React.ReactNode;
  children: React.ReactNode;
}> = ({ icon: Icon, iconClassName, title, subtitle, headerRight, children }) => (
  <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-4 space-y-3">
    <div className="flex items-center justify-between gap-2 pb-2 border-b border-slate-800/80">
      <div className="flex items-center gap-2 min-w-0">
        <Icon className={`w-4 h-4 shrink-0 ${iconClassName}`} />
        <div className="min-w-0">
          <h2 className="text-sm font-bold text-slate-100 tracking-tight">{title}</h2>
          <p className="text-[11px] text-content-muted">{subtitle}</p>
        </div>
      </div>
      {headerRight}
    </div>
    {children}
  </div>
);

/** Renders one run's real task list as a dependency-ordered list, showing
 *  each task's real prerequisite edges. Deliberately a list rather than a
 *  drawn graph: the backend's own model is `prerequisites: TaskId[]`, and a
 *  rendered node/edge canvas would imply layout/ordering information the
 *  scheduler does not actually publish. */
const TaskDag: React.FC<{ tasks: TaskState[] }> = ({ tasks }) => (
  <ul className="space-y-1">
    {tasks.map((task) => (
      <li
        key={task.id}
        className="flex items-start gap-2 px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800"
      >
        <Pill label={task.status} className={taskStatusStyles[task.status]} />
        <div className="min-w-0 flex-1">
          <div className="text-xs text-slate-200 truncate">{task.name}</div>
          <div className="text-[10px] font-mono text-content-muted flex flex-wrap items-center gap-x-2">
            <span>{task.id}</span>
            {task.prerequisites.length > 0 && (
              <span className="text-content-muted">after: {task.prerequisites.join(', ')}</span>
            )}
            {task.reason && <span className="text-rose-400/80">{task.reason}</span>}
          </div>
        </div>
      </li>
    ))}
  </ul>
);

export const AIQAOrchestratorPage: React.FC = () => {
  const navigate = useNavigate();
  const auth = useAuth();
  const realtime = useRealtime();
  const { selectedProject } = useProject();

  const [aiStatus, setAiStatus] = useState<AiStatusResponse | null>(null);
  const [orchState, setOrchState] = useState<OrchestratorStateResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<ApiError | null>(null);
  const [lastUpdated, setLastUpdated] = useState<Date | null>(null);

  const isUnmounted = useRef(false);

  const load = useCallback(async (signal?: AbortSignal) => {
    try {
      const [status, orchestrator] = await Promise.all([
        aiqaApi.ai.status(signal),
        aiqaApi.orchestrator.state(signal),
      ]);
      if (signal?.aborted || isUnmounted.current) return;
      setAiStatus(status);
      setOrchState(orchestrator);
      setError(null);
      setLastUpdated(new Date());
    } catch (err) {
      if (signal?.aborted || isUnmounted.current) return;
      // A poll failure must never leave stale numbers on screen looking
      // current — the error state replaces them outright.
      if (err instanceof ApiError) setError(err);
      setAiStatus(null);
      setOrchState(null);
    } finally {
      if (!signal?.aborted && !isUnmounted.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    isUnmounted.current = false;
    if (auth.status !== 'authenticated') {
      setLoading(false);
      return;
    }
    const controller = new AbortController();
    void load(controller.signal);
    // Provider health only — see `HEALTH_POLL_MS`. Run state is not waiting
    // on this timer; it is refetched the instant a real event arrives below.
    const timer = setInterval(() => void load(controller.signal), HEALTH_POLL_MS);
    return () => {
      isUnmounted.current = true;
      controller.abort();
      clearInterval(timer);
    };
  }, [auth.status, load]);

  // Genuinely event-driven: every real run/task event on the selected
  // project's WebSocket stream (`RealtimeContext`, one shared socket) makes
  // the live DAG refetch immediately. This is what makes the DAG realtime
  // rather than merely polled — a task flipping to RUNNING/PASSED lands here
  // as fast as the backend publishes it, not on the next tick of a timer.
  useEffect(() => {
    if (auth.status !== 'authenticated') return;
    return realtime.subscribe(() => {
      void load();
    });
  }, [auth.status, realtime, load]);

  if (auth.status !== 'authenticated') {
    return (
      <div id="aiqa-orchestrator-page" className="space-y-4 animate-in fade-in duration-150">
        <SectionHeader
          title="AIQA Orchestrator"
          subtitle="Live AI routing, executor capability routing, and run coordination"
          badge="ORCHESTRATION"
          icon={Workflow}
        />
        {renderAuthState(auth)}
      </div>
    );
  }

  const routing = aiStatus?.routing_preview ?? null;

  return (
    <div id="aiqa-orchestrator-page" className="space-y-4 animate-in fade-in duration-150">
      <SectionHeader
        title="AIQA Orchestrator"
        subtitle="Live AI routing, executor capability routing, and run coordination"
        badge="ORCHESTRATION"
        icon={Workflow}
        actions={
          <div className="flex items-center gap-2">
            {lastUpdated && (
              <span className="text-[10px] font-mono text-content-muted">
                updated {lastUpdated.toTimeString().slice(0, 8)}
              </span>
            )}
            <button
              type="button"
              id="btn-orchestrator-refresh"
              onClick={() => void load()}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-[#020617] border border-slate-700 hover:border-violet-600 text-slate-300 hover:text-violet-300 font-mono text-xs font-semibold cursor-pointer transition-colors"
            >
              <RefreshCw className="w-3.5 h-3.5" />
              Refresh
            </button>
            <button
              type="button"
              onClick={() => navigate('/execution')}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 text-white font-semibold text-xs cursor-pointer"
            >
              <PlayCircle className="w-3.5 h-3.5" />
              Execution Hub
            </button>
          </div>
        }
      />

      {loading && !aiStatus && !error && (
        <div className="rounded-lg border border-slate-800 bg-[#0B1120] p-6 flex items-center justify-center gap-2 text-slate-400 text-xs font-mono">
          <Loader2 className="w-4 h-4 animate-spin text-violet-400" />
          Loading real orchestrator state…
        </div>
      )}

      {error &&
        renderResourceErrorState(error, () => void load(), {
          forbiddenTitle: 'Not permitted to view orchestrator state',
          forbiddenDescription:
            'This key is valid but is not permitted to read the orchestrator state or AI routing status.',
          offlineDescriptionSuffix: 'to load live orchestrator state.',
          genericTitle: "Couldn't load orchestrator state",
        })}

      {!error && aiStatus && orchState && (
        // `lg:grid-cols-12` (`col-span-10` main / `col-span-2` rail): the
        // same layout idiom `TestCaseGeneratorPage`'s own generation-flow
        // rail uses, so the previously-empty margin at wide viewports
        // holds a real, live glance at this page's own routing/executor/
        // run state instead of dead space.
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-4 items-start">
        <div className="lg:col-span-9 space-y-4 min-w-0">
          {/* The real execution loop, end to end — including the stages
              this backend genuinely does not have, reported as such. */}
          <NeuralPipeline projectId={selectedProject?.id ?? null} />

          <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
            {/* ---------------------------------------- AI provider routing */}
            <PanelCard
              icon={Network}
              iconClassName="text-yellow-400"
              title="AI Provider Routing"
              subtitle="Which provider/model wins right now, and why"
              headerRight={
                // Honest: `ai_router.rs` re-discovers on a TTL and never
                // pushes, so this lane really is polled. Saying "live" here
                // would claim a feed the backend does not have.
                <Pill label={`polled ${HEALTH_POLL_MS / 1000}s`} />
              }
            >
              {routing ? (
                <div className="rounded-md bg-[#020617] border border-violet-800/70 p-3 space-y-2">
                  <div className="flex items-center justify-between gap-2 flex-wrap">
                    <span className="text-xs font-mono font-bold text-violet-300">
                      {routing.provider_id} / {routing.model_id}
                    </span>
                    <Pill label={`${aiStatus.settings.mode} mode`} className="text-violet-300 bg-violet-950/60 border-violet-800" />
                  </div>
                  <p className="text-[11px] text-slate-400">{routing.reason}</p>
                  <div className="flex items-center gap-2 flex-wrap">
                    <span className="text-[10px] font-mono text-content-muted">
                      routing score {routing.routing_score.toFixed(2)}
                    </span>
                    <Pill
                      label={aiStatus.settings.fallback_enabled ? 'fallback on' : 'fallback off'}
                      className={
                        aiStatus.settings.fallback_enabled
                          ? 'text-emerald-400 bg-emerald-950/60 border-emerald-800'
                          : 'text-slate-400 bg-slate-800/60 border-slate-700'
                      }
                    />
                  </div>
                </div>
              ) : (
                <div className="rounded-md bg-[#020617] border border-slate-800 p-3 flex items-start gap-2">
                  <CircleSlash className="w-3.5 h-3.5 text-content-muted shrink-0 mt-0.5" />
                  <p className="text-[11px] text-slate-400">
                    No compatible model is currently available for test-case generation. No route is shown
                    rather than a guessed one.
                  </p>
                </div>
              )}

              <ul className="space-y-1">
                {aiStatus.providers.map((provider) => (
                  <li
                    key={provider.id}
                    className="flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800"
                  >
                    <div className="min-w-0">
                      <div className="text-xs text-slate-200 truncate">{provider.name}</div>
                      <div className="text-[10px] font-mono text-content-muted">
                        {provider.models.length} model{provider.models.length === 1 ? '' : 's'} discovered
                      </div>
                    </div>
                    <Pill label={provider.status} className={providerStatusStyles[provider.status]} />
                  </li>
                ))}
              </ul>

              <p className="text-[10px] font-mono text-content-muted flex items-start gap-1.5">
                <Info className="w-3 h-3 shrink-0 mt-0.5" />
                <span>
                  Routing score is a documented capability/health heuristic — not an accuracy or quality
                  benchmark.
                </span>
              </p>
            </PanelCard>

            {/* ------------------------------ executor routing + policy gates */}
            <PanelCard
              icon={Cpu}
              iconClassName="text-violet-400"
              title="Executor Capability Routing"
              subtitle={`Policy-enforced max concurrency: ${orchState.max_concurrency}`}
            >
              <ul className="space-y-1">
                {orchState.executors.map((executor) => (
                  <li
                    key={executor.kind}
                    className="flex items-center justify-between gap-2 px-2.5 py-1.5 rounded-md bg-[#020617] border border-slate-800"
                  >
                    <span className="text-xs font-mono text-slate-200 truncate">{executor.kind}</span>
                    <Pill label={executor.gate.replace(/_/g, ' ')} className={gateStyles[executor.gate]} />
                  </li>
                ))}
              </ul>
              <p className="text-[10px] font-mono text-content-muted flex items-start gap-1.5">
                <ShieldAlert className="w-3 h-3 shrink-0 mt-0.5" />
                <span>
                  Gates are the real rules evaluated before scheduling — a gated executor sends its run to
                  PENDING_APPROVAL, a blocked one is denied outright.
                </span>
              </p>
            </PanelCard>
          </div>

          {/* ------------------------------------------------- live DAG */}
          <PanelCard
            icon={GitBranch}
            iconClassName="text-emerald-400"
            title="Live Run Coordination"
            subtitle="Real task states and prerequisite edges for in-process runs"
            headerRight={
              // The real WebSocket state from `RealtimeContext`, not a
              // decorative "live" badge: this says whether the DAG below is
              // actually being pushed updates right now. `disconnected` with
              // no project selected is the truth — that socket is per-project.
              <div className="flex items-center gap-1.5">
                <Pill
                  label={realtime.connectionState === 'connected' ? 'push: live' : `push: ${realtime.connectionState}`}
                  className={
                    realtime.connectionState === 'connected'
                      ? 'text-emerald-400 bg-emerald-950/60 border-emerald-800'
                      : 'text-slate-400 bg-slate-800/60 border-slate-700'
                  }
                />
                {realtime.lastEventAt && (
                  <span className="text-[10px] font-mono text-content-muted">
                    last event {new Date(realtime.lastEventAt).toTimeString().slice(0, 8)}
                  </span>
                )}
              </div>
            }
          >
            {orchState.active_runs.length === 0 ? (
              <div className="rounded-md bg-[#020617] border border-slate-800 p-3 flex items-start gap-2">
                <CheckCircle2 className="w-3.5 h-3.5 text-content-muted shrink-0 mt-0.5" />
                <p className="text-[11px] text-slate-400">
                  No runs are currently being driven by this backend process. Runs that finished before the
                  backend last restarted are history — open them in the Execution Hub.
                </p>
              </div>
            ) : (
              <div className="space-y-3">
                {orchState.active_runs.map((run) => (
                  <div key={run.run_id} className="space-y-1.5">
                    <div className="flex items-center gap-2 flex-wrap">
                      <span className="text-xs font-semibold text-slate-100">{run.name}</span>
                      <Pill label={run.state} className={taskStatusStyles[run.state]} />
                      <span className="text-[10px] font-mono text-content-muted">
                        concurrency {run.concurrency} · {run.tasks.length} task
                        {run.tasks.length === 1 ? '' : 's'}
                      </span>
                    </div>
                    <TaskDag tasks={run.tasks} />
                  </div>
                ))}
              </div>
            )}
          </PanelCard>

          {/* ------------------------------------- realtime activity stream */}
          <OrchestratorLogsPanel projectId={selectedProject?.id ?? null} />
        </div>

        <div className="hidden lg:block lg:col-span-3 min-w-0">
          <OrchestratorRadar orchState={orchState} />
        </div>
        </div>
      )}
    </div>
  );
};
