/**
 * Phase 7 — one autopilot run: what it is, where it has got to, and what the person looking at it can do next.
 *
 * What the page offers follows from the run's state (`runPhase`): a run that has only been made shows its estimate and the two
 * things to accept before it starts; a running one can be cancelled; one waiting at the approval gate shows its cases; a failed
 * one says why and can be started again where it stopped; an ended one shows its report. Every action returns the run as it then
 * is (`AutopilotRunDetail`), which the page shows at once instead of waiting for its next look.
 */
import React, { useMemo, useState } from 'react';
import { AlertTriangle, CheckCircle2, Circle, CircleDashed, Loader2, Play, RotateCcw, Square, XCircle } from 'lucide-react';
import { aiqaApi } from '../../services/aiqaApi';
import { useProviderNames } from '../../hooks/useProviderNames';
import type { AutopilotEvent, AutopilotRunDetail, AutopilotStep, AutopilotStepState } from '../../types/contract';
import { EstimatePanel } from './EstimatePanel';
import { GatePanel } from './GatePanel';
import { ReportView } from './ReportView';
import {
  RUN_STATE_LABEL,
  STEP_LABEL,
  acceptanceState,
  errorText,
  eventTone,
  focusStep,
  formatSeconds,
  formatTimestamp,
  isEnded,
  runPhase,
  secondsBetween,
  stepSummary,
  suiteIdOf,
  withProviderNames,
  type Tone,
} from './autopilotModel';

const STATE_BADGE: Record<string, string> = {
  AWAITING_APPROVAL: 'text-sky-300 bg-sky-950/40 border-sky-800/60',
  RUNNING: 'text-violet-300 bg-violet-950/40 border-violet-800/60',
  WAITING: 'text-amber-300 bg-amber-950/40 border-amber-800/60',
  COMPLETED: 'text-emerald-300 bg-emerald-950/40 border-emerald-800/60',
  FAILED: 'text-rose-300 bg-rose-950/40 border-rose-800/60',
  CANCELLED: 'text-slate-300 bg-slate-800/60 border-slate-700',
};
const TONE_TEXT: Record<Tone, string> = { good: 'text-emerald-300', warn: 'text-amber-300', bad: 'text-rose-300', plain: 'text-slate-300' };
const SHOWN_EVENTS = 40;

const StepIcon: React.FC<{ state: AutopilotStepState }> = ({ state }) => {
  switch (state) {
    case 'DONE':
      return <CheckCircle2 className="w-4 h-4 text-emerald-400" aria-hidden />;
    case 'RUNNING':
      return <Loader2 className="w-4 h-4 text-violet-400 animate-spin" aria-hidden />;
    case 'WAITING':
      return <CircleDashed className="w-4 h-4 text-amber-400" aria-hidden />;
    case 'FAILED':
      return <XCircle className="w-4 h-4 text-rose-400" aria-hidden />;
    case 'SKIPPED':
      return <Circle className="w-4 h-4 text-content-muted" aria-hidden />;
    default:
      return <Circle className="w-4 h-4 text-slate-700" aria-hidden />;
  }
};

const StepList: React.FC<{ steps: AutopilotStep[] }> = ({ steps }) => {
  const focus = focusStep(steps)?.name ?? null;
  const ordered = useMemo(() => [...steps].sort((a, b) => a.position - b.position), [steps]);
  return (
    <ol id="autopilot-steps" className="space-y-1">
      {ordered.map((s) => {
        const summary = stepSummary(s);
        const seconds = secondsBetween(s.started_at, s.finished_at);
        return (
          <li
            key={s.name}
            id={`step-${s.name}`}
            data-state={s.state}
            className={`flex items-start gap-2 p-2 rounded border ${focus === s.name ? 'bg-[#020617] border-slate-700' : 'border-transparent'}`}
          >
            <span className="mt-0.5">
              <StepIcon state={s.state} />
            </span>
            <div className="min-w-0 space-y-0.5">
              <div className="flex flex-wrap items-baseline gap-x-2">
                <span className="text-xs text-slate-100">{STEP_LABEL[s.name]?.title ?? s.name}</span>
                <span className="font-mono text-[10px] text-content-muted">{s.state.toLowerCase()}</span>
                {s.attempts > 1 && <span className="font-mono text-[10px] text-amber-400">attempt {s.attempts}</span>}
                {seconds !== null && <span className="font-mono text-[10px] text-content-muted">{formatSeconds(seconds)}</span>}
              </div>
              {summary && <div className="text-[11px] text-slate-400">{summary}</div>}
              {!summary && s.state === 'PENDING' && <div className="text-[11px] text-content-muted">{STEP_LABEL[s.name]?.what}</div>}
              {s.error && <div className="text-[11px] text-rose-300 break-words">{s.error}</div>}
            </div>
          </li>
        );
      })}
    </ol>
  );
};

const Timeline: React.FC<{ events: AutopilotEvent[]; providerName: (id: string) => string }> = ({ events, providerName }) => {
  const [all, setAll] = useState(false);
  const shown = all ? events : events.slice(-SHOWN_EVENTS);
  return (
    <section id="autopilot-timeline" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 space-y-2">
      <div className="flex items-center gap-2">
        <h2 className="text-xs font-semibold text-slate-100">What happened</h2>
        {events.length > SHOWN_EVENTS && (
          <button
            type="button"
            id="timeline-toggle"
            onClick={() => setAll((v) => !v)}
            className="ml-auto px-2 py-0.5 rounded border border-slate-700 text-[11px] text-slate-300 hover:bg-slate-800 cursor-pointer"
          >
            {all ? `Only the latest ${SHOWN_EVENTS}` : `Show all ${events.length}`}
          </button>
        )}
      </div>
      {events.length === 0 ? (
        <p className="text-[11px] text-content-muted">Nothing has happened yet.</p>
      ) : (
        <ul className="space-y-0.5 max-h-72 overflow-y-auto pr-1">
          {shown.map((e) => (
            <li key={e.id} id={`event-${e.id}`} className="flex gap-2 text-[11px]">
              <span className="font-mono text-content-muted shrink-0">{formatTimestamp(e.at).slice(11)}</span>
              <span className={`${TONE_TEXT[eventTone(e.kind)]} break-words`}>
                {e.step && <span className="font-mono text-content-muted">{STEP_LABEL[e.step]?.title ?? e.step}: </span>}
                {withProviderNames(e.message, providerName)}
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
};

interface Props {
  projectId: string;
  detail: AutopilotRunDetail;
  /** Every event seen so far, oldest first (the page merges what each look at the run brings). */
  events: AutopilotEvent[];
  /** An action's answer: the run as it now is. */
  onDetail: (detail: AutopilotRunDetail) => void;
}

export const RunView: React.FC<Props> = ({ projectId, detail, events, onDetail }) => {
  const { run, steps } = detail;
  const phase = runPhase(detail);
  const [acceptedEstimate, setAcceptedEstimate] = useState(false);
  const [acceptedPaid, setAcceptedPaid] = useState(false);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const acceptance = acceptanceState(run.estimate, acceptedEstimate, acceptedPaid);
  // The router names a provider by its id in a timeline line or a note: a custom one shows the name it was given.
  const providerName = useProviderNames(true);

  const act = async (what: string, action: () => Promise<void>) => {
    setBusy(what);
    setError(null);
    setNotice(null);
    try {
      await action();
    } catch (e) {
      setError(errorText(e, 'That did not work.'));
    } finally {
      setBusy(null);
    }
  };

  const start = () =>
    act('start', async () => {
      onDetail(
        await aiqaApi.autopilot.runs.start(projectId, run.id, {
          accept_estimate: true,
          ...(acceptance.paid && acceptedPaid ? { accept_paid: true } : {}),
        }),
      );
    });
  const cancel = () =>
    act('cancel', async () => {
      const answer = await aiqaApi.autopilot.runs.cancel(projectId, run.id);
      if (answer.outcome === 'requested') setNotice('Cancelling: the run stops as soon as the call it is making has finished.');
      onDetail(answer);
    });
  const resume = () =>
    act('resume', async () => {
      onDetail(await aiqaApi.autopilot.runs.resume(projectId, run.id));
    });

  const took = secondsBetween(run.started_at, run.finished_at);
  const cancelling = run.cancel_requested && !isEnded(run.state);
  // A report is fetched again whenever the run's state or a step's does.
  const version = `${run.state}|${steps.map((s) => s.state).join(',')}`;

  return (
    <div id="autopilot-run" className="space-y-4">
      <header id="run-header" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-1.5">
        <div className="flex flex-wrap items-center gap-2">
          <h2 className="text-sm font-semibold text-slate-100">{run.name}</h2>
          <span id="run-state" className={`px-2 py-0.5 rounded border text-[10px] font-mono uppercase ${STATE_BADGE[run.state] ?? STATE_BADGE.CANCELLED}`}>
            {RUN_STATE_LABEL[run.state]}
          </span>
          {detail.working && (
            <span id="run-working" className="inline-flex items-center gap-1 text-[11px] text-violet-300">
              <Loader2 className="w-3 h-3 animate-spin" /> working
            </span>
          )}
        </div>
        <p className="text-[11px] text-slate-400">
          {run.config.base_url} · made by {run.created_by_name} · {formatTimestamp(run.created_at)}
          {run.started_at ? ` · started ${formatTimestamp(run.started_at)}` : ''}
          {took !== null ? ` · took ${formatSeconds(took)}` : ''}
        </p>
        {run.approved_at && (
          <p id="run-accepted" className="text-[10px] text-content-muted">
            The estimate was accepted {formatTimestamp(run.approved_at)}
            {run.approved_paid ? ', including its cost.' : '.'}
          </p>
        )}
      </header>

      {error && (
        <p id="run-error" role="alert" className="p-2 rounded bg-rose-950/30 border border-rose-800/60 text-[11px] text-rose-200">
          {error}
        </p>
      )}
      {notice && (
        <p id="run-notice" className="p-2 rounded bg-[#020617] border border-slate-800 text-[11px] text-slate-300">
          {notice}
        </p>
      )}

      {phase === 'estimate' && (
        <section id="run-estimate" className="p-4 rounded-lg bg-[#0B1120] border border-sky-800/50 shadow-sm space-y-3">
          <div>
            <h2 className="text-xs font-semibold text-sky-200">Read what this run would cost, then start it</h2>
            <p className="text-[11px] text-slate-400">The run is made and stored, but nothing has been asked of any provider yet.</p>
          </div>
          <EstimatePanel estimate={run.estimate} />
          <div className="space-y-1.5">
            <label className="flex items-start gap-2 text-xs text-slate-200 cursor-pointer">
              <input id="accept-estimate" type="checkbox" checked={acceptedEstimate} onChange={(e) => setAcceptedEstimate(e.target.checked)} className="accent-violet-500 mt-0.5" />
              <span>I have read what this run would cost and take.</span>
            </label>
            {acceptance.paid && (
              <label className="flex items-start gap-2 text-xs text-amber-200 cursor-pointer">
                <input id="accept-paid" type="checkbox" checked={acceptedPaid} onChange={(e) => setAcceptedPaid(e.target.checked)} className="accent-amber-500 mt-0.5" />
                <span>This run would call a provider that charges, and I accept that cost.</span>
              </label>
            )}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <button
              type="button"
              id="run-start"
              onClick={() => void start()}
              disabled={!acceptance.canStart || busy !== null}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 text-white text-xs font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {busy === 'start' ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Play className="w-3.5 h-3.5" />}
              Start the run
            </button>
            <button
              type="button"
              id="run-cancel"
              onClick={() => void cancel()}
              disabled={busy !== null}
              className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-slate-700 text-slate-300 hover:bg-slate-800 text-xs cursor-pointer disabled:opacity-50"
            >
              Discard this run
            </button>
            {acceptance.missing && (
              <span id="run-missing" className="text-[11px] text-content-muted">
                {acceptance.missing}
              </span>
            )}
          </div>
        </section>
      )}

      {phase === 'running' && (
        <section id="run-running" className="p-4 rounded-lg bg-[#0B1120] border border-violet-800/50 shadow-sm space-y-2">
          <p className="flex items-center gap-2 text-xs text-violet-200">
            <Loader2 className="w-4 h-4 animate-spin" />
            {run.current_step ? `Working on: ${STEP_LABEL[run.current_step]?.title ?? run.current_step}` : 'Working'}
          </p>
          <p className="text-[11px] text-slate-400">You can close this page: the run goes on in the service, and picks up where it was if the service restarts.</p>
          {cancelling && (
            <p id="run-cancelling" className="text-[11px] text-amber-300">
              Cancelling: the run stops as soon as the call it is making has finished.
            </p>
          )}
          <button
            type="button"
            id="run-cancel"
            onClick={() => void cancel()}
            disabled={busy !== null || cancelling}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-rose-800 text-rose-300 hover:bg-rose-950/40 text-xs cursor-pointer disabled:opacity-50"
          >
            <Square className="w-3.5 h-3.5" /> Cancel the run
          </button>
        </section>
      )}

      {phase === 'gate' && (
        <GatePanel projectId={projectId} runId={run.id} suiteId={suiteIdOf(steps)} onDetail={onDetail} />
      )}

      {phase === 'failed' && (
        <section id="run-failed" className="p-4 rounded-lg bg-[#0B1120] border border-rose-800/60 shadow-sm space-y-2">
          <h2 className="flex items-center gap-1.5 text-xs font-semibold text-rose-200">
            <AlertTriangle className="w-4 h-4" /> The run stopped
          </h2>
          {run.error && <p className="text-[11px] text-rose-200 break-words">{run.error}</p>}
          <p className="text-[11px] text-slate-400">
            What was done before the step that failed is kept: cases already written stay saved, and a crawl already made is used again. Starting the run again
            takes it up at {run.current_step ? (STEP_LABEL[run.current_step]?.title ?? run.current_step) : 'the step that failed'}; from then on it acts as you.
          </p>
          <button
            type="button"
            id="run-resume"
            onClick={() => void resume()}
            disabled={busy !== null}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 text-white text-xs font-semibold cursor-pointer disabled:opacity-50"
          >
            {busy === 'resume' ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <RotateCcw className="w-3.5 h-3.5" />}
            Start it again where it stopped
          </button>
        </section>
      )}

      {phase === 'cancelled' && (
        <p id="run-cancelled" className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 text-[11px] text-slate-300">
          This run was cancelled{run.finished_at ? ` at ${formatTimestamp(run.finished_at)}` : ''}. What it did before that stays where it was made: the cases it wrote
          are still in the project.
        </p>
      )}

      <section id="run-steps" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 space-y-1">
        <h2 className="text-xs font-semibold text-slate-100">Steps</h2>
        <StepList steps={steps} />
      </section>

      {phase !== 'estimate' && <ReportView projectId={projectId} runId={run.id} autoLoad={isEnded(run.state)} version={version} providerName={providerName} />}

      <Timeline events={events} providerName={providerName} />
    </div>
  );
};
