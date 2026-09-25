/**
 * Phase 7 — Autopilot: from the uploaded requirements to a report of executed tests, in one run.
 *
 * Backend contract (`services/orchestrator/src/autopilot*.rs`, routes under `/api/v1/projects/:project_id/autopilot`): a person
 * sets a run up here (the application, the documents, who to log in as, how much to generate), sees what it would cost as ranges
 * (calls, tokens, time, dollars per provider), and accepts that before anything starts. From then on the run belongs to the
 * service: it crawls, generates, validates, waits at a gate for a person to approve the cases, runs them and writes the report,
 * keeping every step in the database, so it survives a restart and can be picked up again after a failure.
 *
 * This page only shows and asks: it holds nothing the service does not. The chosen run is in the address (`?run=`), so a link,
 * a reload or another day's visit lands on the same run.
 */

import React, { useEffect, useMemo, useRef } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { FolderOpen, Plus, Rocket } from 'lucide-react';
import { useProject } from '../../context/ProjectContext';
import { useAutopilotRun, useAutopilotRuns } from '../../hooks/useAutopilotRun';
import { StateCard, renderResourceErrorState } from '../state/StateViews';
import { EmptyState } from '../common/EmptyState';
import { NewRunWizard } from '../autopilot/NewRunWizard';
import { RunView } from '../autopilot/RunView';
import { RUN_STATE_LABEL, formatTimestamp, isEnded } from '../autopilot/autopilotModel';
import type { AutopilotRun, AutopilotRunDetail } from '../../types/contract';

const STATE_DOT: Record<string, string> = {
  AWAITING_APPROVAL: 'bg-sky-400',
  RUNNING: 'bg-violet-400 animate-pulse',
  WAITING: 'bg-amber-400',
  COMPLETED: 'bg-emerald-400',
  FAILED: 'bg-rose-400',
  CANCELLED: 'bg-slate-500',
};

const RunList: React.FC<{ runs: AutopilotRun[]; chosen: string | null; onChoose: (id: string) => void; onNew: () => void; newChosen: boolean }> = ({
  runs,
  chosen,
  onChoose,
  onNew,
  newChosen,
}) => (
  <nav id="autopilot-runs" aria-label="Autopilot runs" className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 space-y-2">
    <button
      type="button"
      id="autopilot-new"
      onClick={onNew}
      aria-pressed={newChosen}
      className={`w-full inline-flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-md border text-xs cursor-pointer ${newChosen ? 'bg-violet-950/60 border-violet-600 text-violet-200' : 'border-slate-700 text-slate-200 hover:bg-slate-800'}`}
    >
      <Plus className="w-3.5 h-3.5" /> New run
    </button>
    {runs.length === 0 ? (
      <p className="text-[11px] text-slate-500">No run yet.</p>
    ) : (
      <ul className="space-y-1">
        {runs.map((r) => (
          <li key={r.id}>
            <button
              type="button"
              id={`autopilot-run-${r.id}`}
              onClick={() => onChoose(r.id)}
              aria-current={chosen === r.id ? 'true' : undefined}
              className={`w-full text-left p-2 rounded border cursor-pointer ${chosen === r.id ? 'bg-[#020617] border-violet-700' : 'border-slate-800 hover:bg-slate-900'}`}
            >
              <div className="flex items-center gap-1.5">
                <span className={`w-2 h-2 rounded-full ${STATE_DOT[r.state] ?? 'bg-slate-500'}`} aria-hidden />
                <span className="text-xs text-slate-100 truncate">{r.name}</span>
              </div>
              <div className="text-[10px] text-slate-500">
                {RUN_STATE_LABEL[r.state]} · {formatTimestamp(r.created_at)}
              </div>
            </button>
          </li>
        ))}
      </ul>
    )}
  </nav>
);

export const AutopilotPage: React.FC = () => {
  const { selectedProjectId, selectedProject } = useProject();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const chosenId = params.get('run');

  const list = useAutopilotRuns(selectedProjectId);
  const look = useAutopilotRun(selectedProjectId, chosenId);

  const choose = (id: string | null) => {
    const next = new URLSearchParams(params);
    if (id) next.set('run', id);
    else next.delete('run');
    setParams(next);
  };

  // One run at a time is active in a project: coming back to the page lands on it, so a run that is going is never lost from sight.
  // Only on arriving: a person who then presses "New run" is not sent back.
  const active = useMemo(() => list.runs?.find((r) => !isEnded(r.state)) ?? null, [list.runs]);
  const landedOn = useRef<string | null>(null);
  useEffect(() => {
    if (!list.runs || landedOn.current === selectedProjectId) return;
    landedOn.current = selectedProjectId;
    if (!chosenId && active) {
      const next = new URLSearchParams(params);
      next.set('run', active.id);
      setParams(next, { replace: true });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [list.runs, selectedProjectId]);

  // A run belongs to its project: choosing another project leaves the run behind (its id in the address would be a 404 there).
  const shownProject = useRef(selectedProjectId);
  useEffect(() => {
    const before = shownProject.current;
    shownProject.current = selectedProjectId;
    if (before !== null && before !== selectedProjectId && params.has('run')) {
      const next = new URLSearchParams(params);
      next.delete('run');
      setParams(next, { replace: true });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedProjectId]);

  // The list shows the run's state: read it again when the chosen run's state changes.
  const chosenState = look.detail?.run.state ?? null;
  const refreshList = list.refresh;
  useEffect(() => {
    if (chosenState) refreshList();
  }, [chosenState, refreshList]);

  const created = (detail: AutopilotRunDetail) => {
    choose(detail.run.id);
    list.refresh();
  };

  const runs = list.runs ?? [];

  const renderRun = () => {
    if (look.loading && !look.detail) {
      return (
        <StateCard
          icon={Rocket}
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
          title="Loading the run"
          description="Reading the run and its steps…"
        />
      );
    }
    if (look.error && !look.detail) {
      return renderResourceErrorState(look.error, look.refresh, {
        forbiddenTitle: 'Not permitted to view this run',
        forbiddenDescription: "This key's project scope does not permit viewing this project's autopilot runs.",
        genericTitle: "Couldn't load the run",
      });
    }
    if (!look.detail) return null;
    return (
      <div className="space-y-3">
        {look.error && (
          <p id="autopilot-look-error" role="alert" className="p-2 rounded bg-amber-950/30 border border-amber-800/60 text-[11px] text-amber-200">
            The latest look at the run failed ({look.error.message}); what is shown is from the one before. It will look again.
          </p>
        )}
        <RunView projectId={selectedProjectId!} detail={look.detail} events={look.events} onDetail={look.apply} />
      </div>
    );
  };

  const renderBody = () => {
    if (!selectedProjectId) {
      return (
        <EmptyState
          icon={FolderOpen}
          title="No project selected"
          description="Choose a project from the selector in the header to run its requirements through the autopilot."
        />
      );
    }
    if (list.loading && !list.runs) {
      return (
        <StateCard
          icon={Rocket}
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
          title="Loading autopilot runs"
          description={`Fetching the runs of ${selectedProject?.name ?? 'this project'}…`}
        />
      );
    }
    if (list.error && !list.runs) {
      return renderResourceErrorState(list.error, list.refresh, {
        forbiddenTitle: 'Not permitted to view this project',
        forbiddenDescription: "This key's project scope does not permit viewing this project's autopilot runs.",
        genericTitle: "Couldn't load the autopilot runs",
      });
    }
    return (
      <div className="grid gap-4 lg:grid-cols-[16rem_minmax(0,1fr)] items-start">
        <RunList runs={runs} chosen={chosenId} onChoose={choose} onNew={() => choose(null)} newChosen={!chosenId} />
        <div className="min-w-0">
          {chosenId ? (
            renderRun()
          ) : (
            <NewRunWizard
              key={selectedProjectId}
              projectId={selectedProjectId}
              projectName={selectedProject?.name}
              onCreated={created}
              onUploadDocuments={() => navigate('/test-generator')}
              blockedBy={active}
              onOpenBlocking={active ? () => choose(active.id) : undefined}
            />
          )}
        </div>
      </div>
    );
  };

  return (
    <div id="aiqa-autopilot-page" className="space-y-4 animate-in fade-in duration-200 text-slate-200">
      <header
        id="autopilot-header"
        className="flex flex-col md:flex-row md:items-center justify-between gap-3 p-4 rounded-lg bg-[#0B1120] border border-slate-800/90 shadow-sm"
      >
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            <span className="p-1.5 rounded bg-violet-950/80 border border-violet-700/60 text-violet-400">
              <Rocket className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-slate-100 font-sans tracking-tight">Autopilot</h1>
            <span className="px-2 py-0.5 rounded bg-violet-950/60 border border-violet-800/60 text-[10px] font-mono text-violet-300 uppercase">
              UPLOAD → REPORT
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans">
            {selectedProject
              ? `Requirements of ${selectedProject.name} to a report of executed tests: you confirm the roles, see the cost first, and approve the cases`
              : 'Requirements to a report of executed tests: you confirm the roles, see the cost first, and approve the cases'}
          </p>
        </div>
      </header>

      <div aria-live="polite">{renderBody()}</div>
    </div>
  );
};
