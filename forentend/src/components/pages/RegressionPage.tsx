/**
 * Phase 8C — Scheduled Regression: the cases a project has approved and run once are run again on a schedule, and each run is compared
 * with the last time. No AI is asked and nothing is compiled again: a run repeats the tasks the cases were last run as.
 *
 * Backend contract (`services/orchestrator/src/regression*.rs`, routes under `/api/v1/projects/:project_id/regression`): schedules are
 * made, changed, paused and removed; a run can be made at once; every run has a report that says, for each case, what it came to now
 * beside what it came to the last time. This page only shows and asks. The open schedule and run are in the address (`?schedule=`,
 * `?run=`), so a link, a reload or another day's visit lands on them.
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { CalendarClock, FolderOpen, Pause, Play, Plus, Trash2 } from 'lucide-react';
import { useProject } from '../../context/ProjectContext';
import { useRegressionReport, useRegressionRuns, useRegressionSchedules } from '../../hooks/useRegression';
import { aiqaApi } from '../../services/aiqaApi';
import { StateCard, renderResourceErrorState } from '../state/StateViews';
import { EmptyState } from '../common/EmptyState';
import { ScheduleForm, type ScopeState } from '../regression/ScheduleForm';
import { Badge, RegressionReportView, type CopyState } from '../regression/RegressionReportView';
import { VERDICT_LABEL, buildBody, ciSnippet, emptyForm, everyText, formProblems, verdictTone, type ScheduleFormState } from '../regression/regressionModel';
import { errorText, formatTimestamp } from '../autopilot/autopilotModel';
import { fileNameOf, saveTextFile } from '../common/saveTextFile';
import type { RegressionRunAccount, RegressionSchedule, TestSuiteRow } from '../../types/contract';

const dot = (s: RegressionSchedule): string => {
  const verdict = s.last_run?.verdict;
  if (verdict === 'RUNNING') return 'bg-violet-400 animate-pulse';
  if (verdict === 'PASSING') return 'bg-emerald-400';
  if (verdict === 'REGRESSED' || verdict === 'FAILING') return 'bg-rose-400';
  return 'bg-slate-500';
};

const ScheduleList: React.FC<{
  schedules: RegressionSchedule[];
  chosen: string | null;
  onChoose: (id: string) => void;
  onNew: () => void;
  loading: boolean;
  error: string | null;
}> = ({ schedules, chosen, onChoose, onNew, loading, error }) => (
  <nav id="regression-schedules" aria-label="Regression schedules" className="min-w-0 p-3 rounded-lg bg-[#0B1120] border border-slate-800 space-y-2">
    <button
      type="button"
      id="regression-new"
      onClick={onNew}
      aria-pressed={chosen === null}
      className={`w-full inline-flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-md border text-xs cursor-pointer ${chosen === null ? 'bg-violet-950/60 border-violet-600 text-violet-200' : 'border-slate-700 text-slate-200 hover:bg-slate-800'}`}
    >
      <Plus className="w-3.5 h-3.5" /> New schedule
    </button>
    {schedules.length === 0 ? (
      loading ? (
        <p className="text-[11px] text-content-muted">Loading…</p>
      ) : error ? (
        <p id="regression-schedules-error" role="alert" className="text-[11px] text-rose-300">
          The schedules could not be read: {error}
        </p>
      ) : (
        <p className="text-[11px] text-content-muted">No schedule yet.</p>
      )
    ) : (
      <ul className="space-y-1">
        {schedules.map((s) => (
          <li key={s.id}>
            <button
              type="button"
              id={`regression-schedule-${s.id}`}
              onClick={() => onChoose(s.id)}
              aria-current={chosen === s.id ? 'true' : undefined}
              className={`w-full text-left p-2 rounded border cursor-pointer ${chosen === s.id ? 'bg-[#020617] border-violet-700' : 'border-slate-800 hover:bg-slate-900'}`}
            >
              <div className="flex items-center gap-1.5">
                <span className={`w-2 h-2 rounded-full ${dot(s)}`} aria-hidden />
                <span className="text-xs text-slate-100 truncate">{s.name}</span>
              </div>
              <div className="text-[10px] text-content-muted">
                {s.enabled ? `${everyText(s.every_minutes)}, next ${formatTimestamp(s.next_run_at)}` : 'paused: runs only when asked'}
              </div>
            </button>
          </li>
        ))}
      </ul>
    )}
  </nav>
);

const HistoryList: React.FC<{ runs: RegressionRunAccount[]; chosen: string | null; onChoose: (id: string) => void }> = ({ runs, chosen, onChoose }) =>
  runs.length === 0 ? (
    <p id="regression-history-empty" className="text-[11px] text-content-muted">
      No run yet.
    </p>
  ) : (
    <ul id="regression-history" className="space-y-1">
      {runs.map((r) => (
        <li key={r.id}>
          <button
            type="button"
            id={`regression-run-${r.id}`}
            onClick={() => onChoose(r.id)}
            aria-current={chosen === r.id ? 'true' : undefined}
            className={`w-full flex flex-wrap items-center gap-2 text-left p-2 rounded border text-[11px] cursor-pointer ${chosen === r.id ? 'bg-[#020617] border-violet-700' : 'border-slate-800 hover:bg-slate-900'}`}
          >
            <span className="text-slate-200">{formatTimestamp(r.created_at)}</span>
            <span className="text-content-muted">{r.trigger === 'SCHEDULE' ? 'schedule' : 'by hand'}</span>
            <Badge tone={verdictTone(r.verdict)}>{VERDICT_LABEL[r.verdict]}</Badge>
            <span className="ml-auto text-slate-400">
              {r.summary.passed} passed · {r.summary.failed + r.summary.error} failed · {r.summary.regressed} regressed
            </span>
          </button>
        </li>
      ))}
    </ul>
  );

type Copy = { state: CopyState; text: string; error: string | null };

export const RegressionPage: React.FC = () => {
  const { selectedProjectId, selectedProject } = useProject();
  const [params, setParams] = useSearchParams();
  const scheduleId = params.get('schedule');
  const runId = params.get('run');

  const schedules = useRegressionSchedules(selectedProjectId);
  const history = useRegressionRuns(selectedProjectId, scheduleId);
  const look = useRegressionReport(selectedProjectId, runId);

  const [form, setForm] = useState<ScheduleFormState>(emptyForm);
  const [suites, setSuites] = useState<TestSuiteRow[]>([]);
  const [scope, setScope] = useState<ScopeState>({ status: 'loading' });
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [copy, setCopy] = useState<Copy>({ state: 'idle', text: '', error: null });
  const [ciCopied, setCiCopied] = useState(false);

  const problems = useMemo(() => formProblems(form), [form]);
  const schedule = scheduleId ? (schedules.value?.find((s) => s.id === scheduleId) ?? null) : null;

  const pick = (next: { schedule?: string | null; run?: string | null }) => {
    const target = new URLSearchParams(params);
    if ('schedule' in next) {
      if (next.schedule) target.set('schedule', next.schedule);
      else target.delete('schedule');
      // Another schedule, another history: its runs are not the open one's.
      target.delete('run');
    }
    if ('run' in next) {
      if (next.run) target.set('run', next.run);
      else target.delete('run');
    }
    setParams(target);
  };

  // A project's schedules belong to it: choosing another project leaves them and what was open behind (their ids in the address would
  // be a 404 there). The first choice (the saved project coming back on a reload) leaves the address alone: it is what a link lands on.
  const shownProject = useRef(selectedProjectId);
  useEffect(() => {
    const before = shownProject.current;
    shownProject.current = selectedProjectId;
    if (before === selectedProjectId) return;
    setCreateError(null);
    setActionError(null);
    setConfirmingDelete(false);
    if (before !== null && (params.has('schedule') || params.has('run'))) {
      const target = new URLSearchParams(params);
      target.delete('schedule');
      target.delete('run');
      setParams(target, { replace: true });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedProjectId]);

  // Another schedule or run is another thing to have been asked about, and another report to copy.
  useEffect(() => {
    setActionError(null);
    setConfirmingDelete(false);
    setCiCopied(false);
  }, [scheduleId]);
  useEffect(() => setCopy({ state: 'idle', text: '', error: null }), [runId]);

  // The project's suites, for choosing what a schedule repeats. Not being able to read them leaves "every case".
  useEffect(() => {
    setSuites([]);
    if (!selectedProjectId) return;
    const controller = new AbortController();
    aiqaApi.testSuites
      .list(selectedProjectId, controller.signal)
      .then((rows) => {
        if (!controller.signal.aborted) setSuites(Array.isArray(rows) ? rows : []);
      })
      .catch(() => undefined);
    return () => controller.abort();
  }, [selectedProjectId]);

  // What the schedule being made would repeat, read again when the suite changes.
  useEffect(() => {
    if (!selectedProjectId || scheduleId) return;
    const controller = new AbortController();
    setScope({ status: 'loading' });
    aiqaApi.regression
      .scope(selectedProjectId, form.suiteId || null, controller.signal)
      .then((found) => {
        if (!controller.signal.aborted) setScope({ status: 'ready', scope: found });
      })
      .catch((e) => {
        if (!controller.signal.aborted) setScope({ status: 'failed', message: errorText(e, 'unknown error') });
      });
    return () => controller.abort();
  }, [selectedProjectId, scheduleId, form.suiteId]);

  const create = () => {
    if (!selectedProjectId || problems.length > 0) return;
    setCreating(true);
    setCreateError(null);
    aiqaApi.regression.schedules
      .create(selectedProjectId, buildBody(form))
      .then((made) => {
        schedules.refresh();
        setForm(emptyForm());
        pick({ schedule: made.id });
      })
      .catch((e) => setCreateError(errorText(e, 'The schedule could not be made.')))
      .finally(() => setCreating(false));
  };

  const act = (what: () => Promise<unknown>, fallback: string, after: () => void) => {
    setBusy(true);
    setActionError(null);
    what()
      .then(after)
      .catch((e) => setActionError(errorText(e, fallback)))
      .finally(() => setBusy(false));
  };

  const runNowThenOpen = (s: RegressionSchedule) => {
    if (!selectedProjectId) return;
    setBusy(true);
    setActionError(null);
    aiqaApi.regression.schedules
      .runNow(selectedProjectId, s.id)
      .then((started) => {
        history.refresh();
        schedules.refresh();
        pick({ run: started.id });
      })
      .catch((e) => setActionError(errorText(e, 'The run could not be made.')))
      .finally(() => setBusy(false));
  };

  const toggle = (s: RegressionSchedule) =>
    act(
      () => aiqaApi.regression.schedules.update(selectedProjectId as string, s.id, { enabled: !s.enabled }),
      'The schedule could not be changed.',
      () => schedules.refresh(),
    );

  const remove = (s: RegressionSchedule) =>
    act(
      () => aiqaApi.regression.schedules.remove(selectedProjectId as string, s.id),
      'The schedule could not be removed.',
      () => {
        setConfirmingDelete(false);
        schedules.refresh();
        pick({ schedule: null });
      },
    );

  const copyMarkdown = useCallback(async () => {
    if (!selectedProjectId || !runId) return;
    try {
      const text = await aiqaApi.regression.runs.markdown(selectedProjectId, runId);
      try {
        await navigator.clipboard.writeText(text);
        setCopy({ state: 'copied', text, error: null });
      } catch {
        // No clipboard (an insecure page, a denied permission): show the text to copy by hand.
        setCopy({ state: 'shown', text, error: null });
      }
    } catch (e) {
      setCopy({ state: 'idle', text: '', error: errorText(e, 'The report could not be read as text.') });
    }
  }, [selectedProjectId, runId]);

  // The report as JUnit XML, saved as a file: what a CI system reads. Made from the report as it is now, so it is what the run has found so far while the run goes.
  const downloadJunit = useCallback(async () => {
    if (!selectedProjectId || !runId) return;
    try {
      const xml = await aiqaApi.regression.runs.junit(selectedProjectId, runId);
      saveTextFile(fileNameOf(`regression-${look.value?.schedule_name ?? 'run'}`, 'xml'), xml);
      setCopy((c) => ({ ...c, error: null }));
    } catch (e) {
      setCopy((c) => ({ ...c, error: errorText(e, 'The report could not be read as JUnit XML.') }));
    }
  }, [selectedProjectId, runId, look.value?.schedule_name]);

  const copyCiSnippet = async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCiCopied(true);
    } catch {
      // No clipboard: the lines are on screen to copy by hand.
    }
  };

  const runsGoing = history.value?.some((r) => !r.final) ?? false;

  const renderReport = () => {
    if (look.loading && !look.value) {
      return (
        <StateCard
          icon={CalendarClock}
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
          title="Loading the report"
          description="Reading what the run has found…"
        />
      );
    }
    if (look.error && !look.value) {
      return renderResourceErrorState(look.error, look.refresh, {
        forbiddenTitle: 'Not permitted to view this run',
        forbiddenDescription: "This key's project scope does not permit viewing this project's regression runs.",
        genericTitle: "Couldn't load the report",
      });
    }
    if (!look.value) return null;
    return (
      <div className="space-y-3">
        {look.error && (
          <p id="regression-look-error" role="alert" className="p-2 rounded bg-amber-950/30 border border-amber-800/60 text-[11px] text-amber-200">
            The latest look at the run failed ({look.error.message}); what is shown is from the one before. It will look again.
          </p>
        )}
        {copy.error && (
          <p id="regression-copy-error" role="alert" className="text-[11px] text-rose-300">
            {copy.error}
          </p>
        )}
        <RegressionReportView report={look.value} onCopyMarkdown={() => void copyMarkdown()} copyState={copy.state} markdown={copy.text} onDownloadJunit={() => void downloadJunit()} />
      </div>
    );
  };

  const renderDetail = (s: RegressionSchedule) => (
    <div id="regression-detail" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-sm font-semibold text-slate-100">{s.name}</h2>
        {!s.enabled && <Badge tone="plain">Paused</Badge>}
        {runsGoing && <Badge tone="warn">A run is going</Badge>}
      </div>
      <p id="regression-facts" className="text-[11px] text-slate-400">
        {everyText(s.every_minutes)} · {s.suite_id ? 'the cases of one suite' : 'every case of the project that has been run once'} · {s.concurrency} at a time ·{' '}
        {s.enabled ? `next run ${formatTimestamp(s.next_run_at)}` : 'runs only when asked'}
      </p>
      {s.last_note && (
        <p id="regression-note" className="text-[11px] text-amber-300">
          {s.last_note}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          id="regression-run-now"
          disabled={busy || runsGoing}
          onClick={() => runNowThenOpen(s)}
          title={runsGoing ? 'The run before has not ended' : undefined}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 text-xs font-semibold text-white hover:bg-violet-500 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
        >
          <Play className="w-3.5 h-3.5" /> {busy ? 'Working…' : 'Run now'}
        </button>
        <button
          type="button"
          id="regression-toggle"
          disabled={busy}
          onClick={() => toggle(s)}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-slate-700 text-xs text-slate-200 hover:bg-slate-800 cursor-pointer disabled:opacity-50"
        >
          {s.enabled ? (
            <>
              <Pause className="w-3.5 h-3.5" /> Pause
            </>
          ) : (
            <>
              <Play className="w-3.5 h-3.5" /> Resume
            </>
          )}
        </button>
        {confirmingDelete ? (
          <span id="regression-delete-confirm" className="inline-flex items-center gap-2 text-[11px] text-rose-200">
            Remove it and its history?
            <button type="button" id="regression-delete-yes" disabled={busy} onClick={() => remove(s)} className="px-2 py-1 rounded border border-rose-700 text-rose-200 hover:bg-rose-950/60 cursor-pointer disabled:opacity-50">
              Remove
            </button>
            <button type="button" id="regression-delete-no" onClick={() => setConfirmingDelete(false)} className="px-2 py-1 rounded border border-slate-700 text-slate-300 hover:bg-slate-800 cursor-pointer">
              Keep
            </button>
          </span>
        ) : (
          <button
            type="button"
            id="regression-delete"
            disabled={busy}
            onClick={() => setConfirmingDelete(true)}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-slate-700 text-xs text-slate-300 hover:bg-slate-800 cursor-pointer disabled:opacity-50"
          >
            <Trash2 className="w-3.5 h-3.5" /> Remove
          </button>
        )}
      </div>
      {actionError && (
        <p id="regression-action-error" role="alert" className="p-2 rounded bg-rose-950/30 border border-rose-800/60 text-[11px] text-rose-200">
          {actionError}
        </p>
      )}
      <details id="regression-ci" className="rounded-lg bg-[#020617] border border-slate-800">
        <summary className="p-2 cursor-pointer text-[11px] text-slate-300">Use it from CI</summary>
        <div className="px-3 pb-3 space-y-2 text-[11px] text-slate-400">
          <p>
            A CI job can run this schedule and fail the build on what it finds: it starts a run, waits for it, saves the JUnit XML for the build&rsquo;s test page and exits with a code the build
            reads.
          </p>
          <pre id="regression-ci-snippet" className="p-2 rounded border border-slate-800 bg-[#0B1120] font-mono text-[11px] text-slate-300 overflow-x-auto whitespace-pre">
            {ciSnippet(selectedProjectId ?? '', s.id)}
          </pre>
          <button
            type="button"
            id="regression-ci-copy"
            onClick={() => void copyCiSnippet(ciSnippet(selectedProjectId ?? '', s.id))}
            className="px-2 py-1 rounded border border-slate-700 text-[11px] text-slate-200 hover:bg-slate-800 cursor-pointer"
          >
            {ciCopied ? 'Copied' : 'Copy these lines'}
          </button>
          <p>
            Exit code 0: the build may go on. 1: it should fail on the result. 2: there is no result (the run could not be started or read, did not end in time, or nothing ran). The key is an API key
            with the operator role for this project, kept as a secret of the CI system. The script is <span className="font-mono">scripts/ci/aiqa-ci.sh</span> (<span className="font-mono">aiqa-ci.ps1</span>{' '}
            on Windows); every setting, and examples for GitHub Actions, GitLab, Jenkins and Azure DevOps, are in <span className="font-mono">docs/ci/README.md</span> of the AIQA repository.
          </p>
        </div>
      </details>
      <div className="space-y-1.5">
        <h3 className="text-xs font-semibold text-slate-100">Runs</h3>
        {history.loading && !history.value ? (
          <p className="text-[11px] text-content-muted">Loading…</p>
        ) : history.error && !history.value ? (
          <p id="regression-history-error" role="alert" className="text-[11px] text-rose-300">
            The runs could not be read: {history.error.message}
          </p>
        ) : (
          <HistoryList runs={history.value ?? []} chosen={runId} onChoose={(id) => pick({ run: id })} />
        )}
      </div>
    </div>
  );

  const renderNew = () => (
    <div className="space-y-4">
      <ScheduleForm form={form} onChange={setForm} problems={problems} suites={suites} scope={scope} onSubmit={create} submitting={creating} />
      {createError && (
        <p id="regression-create-error" role="alert" className="p-2 rounded bg-rose-950/30 border border-rose-800/60 text-[11px] text-rose-200">
          {createError}
        </p>
      )}
    </div>
  );

  const renderBody = () => {
    if (!selectedProjectId) {
      return (
        <EmptyState
          icon={FolderOpen}
          title="No project selected"
          description="Choose a project from the selector in the header to run its tested cases again on a schedule."
        />
      );
    }
    const listedNow = schedules.value !== null;
    return (
      <div className="grid gap-4 grid-cols-1 lg:grid-cols-[16rem_minmax(0,1fr)] items-start">
        <ScheduleList
          schedules={schedules.value ?? []}
          chosen={scheduleId}
          onChoose={(id) => pick({ schedule: id })}
          onNew={() => pick({ schedule: null })}
          loading={schedules.loading && !schedules.value}
          error={schedules.value ? null : (schedules.error?.message ?? null)}
        />
        <div className="min-w-0 space-y-4">
          {!scheduleId ? (
            runId ? null : (
              renderNew()
            )
          ) : schedule ? (
            renderDetail(schedule)
          ) : listedNow ? (
            <p id="regression-missing" className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 text-xs text-slate-300">
              This schedule is not in this project (any more).
            </p>
          ) : null}
          {runId && renderReport()}
        </div>
      </div>
    );
  };

  return (
    <div id="aiqa-regression-page" className="space-y-4 animate-in fade-in duration-200 text-slate-200">
      <header
        id="regression-header"
        className="flex flex-col md:flex-row md:items-center justify-between gap-3 p-4 rounded-lg bg-[#0B1120] border border-slate-800/90 shadow-sm"
      >
        <div className="space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="p-1.5 rounded bg-violet-950/80 border border-violet-700/60 text-violet-400">
              <CalendarClock className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-slate-100 font-sans tracking-tight">Scheduled Regression</h1>
            <span className="px-2 py-0.5 rounded bg-violet-950/60 border border-violet-800/60 text-[10px] font-mono text-violet-300 uppercase">
              RUN AGAIN → COMPARE
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans">
            {selectedProject
              ? `The cases of ${selectedProject.name} that have been run once, run again on a schedule and compared with the last time: no AI, nothing compiled again`
              : 'The cases a project has run once, run again on a schedule and compared with the last time: no AI, nothing compiled again'}
          </p>
        </div>
      </header>

      <div>{renderBody()}</div>
    </div>
  );
};
