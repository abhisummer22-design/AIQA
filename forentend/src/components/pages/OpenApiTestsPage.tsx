/**
 * Phase 8B — OpenAPI Tests: contract tests made from an API's own document, by rule and with no AI.
 *
 * Backend contract (`services/orchestrator/src/openapi_*.rs`, routes under `/api/v1/projects/:project_id/openapi`): a person gives the
 * OpenAPI/Swagger document and where the API is; the service works out the cases the document lets one expect an answer to (a valid
 * request, one without a credential, one with a required parameter left out, a value of the wrong kind, an id that is not there), shows
 * them, and on request runs them as one execution and reports, operation by operation, what was expected and what came back. A request
 * that changes data is run only when asked for, and a credential is only a reference to a variable of the orchestrator's environment.
 *
 * This page only shows and asks. The chosen run is in the address (`?run=`), so a link, a reload or another day's visit lands on it.
 */

import React, { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { FileJson, FolderOpen, Plus } from 'lucide-react';
import { useProject } from '../../context/ProjectContext';
import { useOpenApiRun, useOpenApiRuns } from '../../hooks/useOpenApiRun';
import { aiqaApi } from '../../services/aiqaApi';
import { StateCard, renderResourceErrorState } from '../state/StateViews';
import { EmptyState } from '../common/EmptyState';
import { OpenApiForm } from '../openapi/OpenApiForm';
import { CasePreview } from '../openapi/CasePreview';
import { OpenApiReportView } from '../openapi/OpenApiReportView';
import { buildBody, emptyForm, formProblems, type OpenApiFormState } from '../openapi/openApiModel';
import { errorText, formatTimestamp } from '../autopilot/autopilotModel';
import { fileNameOf, saveTextFile } from '../common/saveTextFile';
import type { OpenApiPlan, OpenApiRunListRow } from '../../types/contract';

const STATE_DOT: Record<string, string> = {
  COMPLETED: 'bg-emerald-400',
  PARTIAL: 'bg-amber-400',
  FAILED: 'bg-rose-400',
  CANCELLED: 'bg-slate-500',
  RUNNING: 'bg-violet-400 animate-pulse',
  SCHEDULED: 'bg-violet-400 animate-pulse',
};

const RunList: React.FC<{
  runs: OpenApiRunListRow[];
  chosen: string | null;
  onChoose: (id: string) => void;
  onNew: () => void;
  loading: boolean;
  /** Why the runs could not be read, when they could not and there are none to show. */
  error: string | null;
}> = ({ runs, chosen, onChoose, onNew, loading, error }) => (
  <nav id="openapi-runs" aria-label="OpenAPI test runs" className="min-w-0 p-3 rounded-lg bg-[#0B1120] border border-slate-800 space-y-2">
    <button
      type="button"
      id="openapi-new"
      onClick={onNew}
      aria-pressed={chosen === null}
      className={`w-full inline-flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-md border text-xs cursor-pointer ${chosen === null ? 'bg-violet-950/60 border-violet-600 text-violet-200' : 'border-slate-700 text-slate-200 hover:bg-slate-800'}`}
    >
      <Plus className="w-3.5 h-3.5" /> New test
    </button>
    {runs.length === 0 ? (
      loading ? (
        <p className="text-[11px] text-content-muted">Loading…</p>
      ) : error ? (
        <p id="openapi-runs-error" role="alert" className="text-[11px] text-rose-300">
          The runs could not be read: {error}
        </p>
      ) : (
        <p className="text-[11px] text-content-muted">No run yet.</p>
      )
    ) : (
      <ul className="space-y-1">
        {runs.map((r) => (
          <li key={r.id}>
            <button
              type="button"
              id={`openapi-run-${r.id}`}
              onClick={() => onChoose(r.id)}
              aria-current={chosen === r.id ? 'true' : undefined}
              className={`w-full text-left p-2 rounded border cursor-pointer ${chosen === r.id ? 'bg-[#020617] border-violet-700' : 'border-slate-800 hover:bg-slate-900'}`}
            >
              <div className="flex items-center gap-1.5">
                <span className={`w-2 h-2 rounded-full ${STATE_DOT[r.run_state ?? ''] ?? 'bg-slate-500'}`} aria-hidden />
                <span className="text-xs text-slate-100 truncate">
                  {r.title} {r.version}
                </span>
              </div>
              <div className="text-[10px] text-content-muted">
                {r.cases_run} of {r.cases} cases · {formatTimestamp(r.created_at)}
              </div>
            </button>
          </li>
        ))}
      </ul>
    )}
  </nav>
);

type PlanState = { status: 'idle' } | { status: 'reading' } | { status: 'failed'; message: string } | { status: 'ready'; plan: OpenApiPlan };

export const OpenApiTestsPage: React.FC = () => {
  const { selectedProjectId, selectedProject } = useProject();
  const [params, setParams] = useSearchParams();
  const chosenId = params.get('run');

  const list = useOpenApiRuns(selectedProjectId);
  const look = useOpenApiRun(selectedProjectId, chosenId);

  const [form, setForm] = useState<OpenApiFormState>(emptyForm);
  const [planState, setPlanState] = useState<PlanState>({ status: 'idle' });
  const [selected, setSelected] = useState<ReadonlySet<string>>(new Set());
  const [starting, setStarting] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);
  const [copy, setCopy] = useState<{ state: 'idle' | 'copied' | 'shown'; text: string; error: string | null }>({ state: 'idle', text: '', error: null });
  const planController = useRef<AbortController | null>(null);

  const problems = useMemo(() => formProblems(form), [form]);

  const choose = (id: string | null) => {
    const next = new URLSearchParams(params);
    if (id) next.set('run', id);
    else next.delete('run');
    setParams(next);
  };

  // A project's tests belong to it: choosing another project leaves the plan and the run behind (its id in the address would be a 404
  // there). The first choice (the saved project coming back on a reload) leaves the address alone: it is what a link to a run lands on.
  const shownProject = useRef(selectedProjectId);
  useEffect(() => {
    const before = shownProject.current;
    shownProject.current = selectedProjectId;
    if (before === selectedProjectId) return;
    planController.current?.abort();
    setPlanState({ status: 'idle' });
    setStartError(null);
    if (before !== null && params.has('run')) {
      const next = new URLSearchParams(params);
      next.delete('run');
      setParams(next, { replace: true });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedProjectId]);

  // Another run is another report to copy.
  useEffect(() => setCopy({ state: 'idle', text: '', error: null }), [chosenId]);

  const changeForm = (next: OpenApiFormState) => {
    planController.current?.abort();
    setForm(next);
    // What was shown was made from the form as it was: it is not what this form would give.
    setPlanState((s) => (s.status === 'idle' ? s : { status: 'idle' }));
    setStartError(null);
  };

  const showCases = () => {
    if (!selectedProjectId) return;
    planController.current?.abort();
    const controller = new AbortController();
    planController.current = controller;
    setPlanState({ status: 'reading' });
    setStartError(null);
    aiqaApi.openapi
      .plan(selectedProjectId, buildBody(form), controller.signal)
      .then((plan) => {
        if (controller.signal.aborted) return;
        setSelected(new Set(plan.operations.filter((op) => plan.cases.some((c) => c.operation === op.id)).map((op) => op.id)));
        setPlanState({ status: 'ready', plan });
      })
      .catch((e) => {
        if (!controller.signal.aborted) setPlanState({ status: 'failed', message: errorText(e, 'The document could not be read.') });
      });
  };

  const run = () => {
    if (!selectedProjectId || planState.status !== 'ready') return;
    const plan = planState.plan;
    const withCases = plan.operations.filter((op) => plan.cases.some((c) => c.operation === op.id)).map((op) => op.id);
    const chosen = withCases.filter((id) => selected.has(id));
    setStarting(true);
    setStartError(null);
    aiqaApi.openapi.runs
      .create(selectedProjectId, buildBody(form, { operations: chosen.length === withCases.length ? [] : chosen }))
      .then((created) => {
        list.refresh();
        choose(created.id);
      })
      .catch((e) => setStartError(errorText(e, 'The run could not be started.')))
      .finally(() => setStarting(false));
  };

  const toggleOperation = (id: string) =>
    setSelected((s) => {
      const next = new Set(s);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  const selectAll = (all: boolean) => {
    if (planState.status !== 'ready') return;
    const plan = planState.plan;
    setSelected(all ? new Set(plan.operations.filter((op) => plan.cases.some((c) => c.operation === op.id)).map((op) => op.id)) : new Set());
  };

  const copyMarkdown = useCallback(async () => {
    if (!selectedProjectId || !chosenId) return;
    try {
      const text = await aiqaApi.openapi.runs.markdown(selectedProjectId, chosenId);
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
  }, [selectedProjectId, chosenId]);

  // The report as JUnit XML, saved as a file: what a CI system reads (Phase 8D).
  const downloadJunit = useCallback(async () => {
    if (!selectedProjectId || !chosenId) return;
    try {
      const xml = await aiqaApi.openapi.runs.junit(selectedProjectId, chosenId);
      saveTextFile(fileNameOf(`api-tests-${look.report?.spec.title ?? 'run'}`, 'xml'), xml);
      setCopy((c) => ({ ...c, error: null }));
    } catch (e) {
      setCopy((c) => ({ ...c, error: errorText(e, 'The report could not be read as JUnit XML.') }));
    }
  }, [selectedProjectId, chosenId, look.report?.spec.title]);

  const renderReport = () => {
    if (look.loading && !look.report) {
      return (
        <StateCard
          icon={FileJson}
          iconClassName="text-violet-400"
          spin
          statusLabel="LOADING…"
          statusClassName="text-violet-300 bg-violet-950/60 border-violet-800"
          title="Loading the report"
          description="Reading what the run has found…"
        />
      );
    }
    if (look.error && !look.report) {
      return renderResourceErrorState(look.error, look.refresh, {
        forbiddenTitle: 'Not permitted to view this run',
        forbiddenDescription: "This key's project scope does not permit viewing this project's API test runs.",
        genericTitle: "Couldn't load the report",
      });
    }
    if (!look.report) return null;
    return (
      <div className="space-y-3">
        {look.error && (
          <p id="openapi-look-error" role="alert" className="p-2 rounded bg-amber-950/30 border border-amber-800/60 text-[11px] text-amber-200">
            The latest look at the run failed ({look.error.message}); what is shown is from the one before. It will look again.
          </p>
        )}
        {copy.error && (
          <p id="openapi-copy-error" role="alert" className="text-[11px] text-rose-300">
            {copy.error}
          </p>
        )}
        <OpenApiReportView report={look.report} onCopyMarkdown={() => void copyMarkdown()} copyState={copy.state} markdown={copy.text} onDownloadJunit={() => void downloadJunit()} />
      </div>
    );
  };

  const renderNew = () => (
    <div className="space-y-4">
      <OpenApiForm form={form} onChange={changeForm} problems={problems} onShowCases={showCases} showing={planState.status === 'reading'} />
      {planState.status === 'failed' && (
        <p id="openapi-plan-error" role="alert" className="p-2 rounded bg-rose-950/30 border border-rose-800/60 text-[11px] text-rose-200">
          {planState.message}
        </p>
      )}
      {startError && (
        <p id="openapi-start-error" role="alert" className="p-2 rounded bg-rose-950/30 border border-rose-800/60 text-[11px] text-rose-200">
          {startError}
        </p>
      )}
      {planState.status === 'ready' && (
        <CasePreview plan={planState.plan} selected={selected} onToggle={toggleOperation} onSelectAll={selectAll} onRun={run} running={starting} unsafeAllowed={form.includeUnsafe} />
      )}
    </div>
  );

  const renderBody = () => {
    if (!selectedProjectId) {
      return (
        <EmptyState
          icon={FolderOpen}
          title="No project selected"
          description="Choose a project from the selector in the header to test its API from the API's own OpenAPI document."
        />
      );
    }
    return (
      <div className="grid gap-4 grid-cols-1 lg:grid-cols-[16rem_minmax(0,1fr)] items-start">
        <RunList
          runs={list.runs ?? []}
          chosen={chosenId}
          onChoose={choose}
          onNew={() => choose(null)}
          loading={list.loading && !list.runs}
          error={list.runs ? null : (list.error?.message ?? null)}
        />
        <div className="min-w-0">{chosenId ? renderReport() : renderNew()}</div>
      </div>
    );
  };

  return (
    <div id="aiqa-openapi-page" className="space-y-4 animate-in fade-in duration-200 text-slate-200">
      <header
        id="openapi-header"
        className="flex flex-col md:flex-row md:items-center justify-between gap-3 p-4 rounded-lg bg-[#0B1120] border border-slate-800/90 shadow-sm"
      >
        <div className="space-y-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="p-1.5 rounded bg-violet-950/80 border border-violet-700/60 text-violet-400">
              <FileJson className="w-4 h-4" />
            </span>
            <h1 className="text-base font-bold text-slate-100 font-sans tracking-tight">OpenAPI Tests</h1>
            <span className="px-2 py-0.5 rounded bg-violet-950/60 border border-violet-800/60 text-[10px] font-mono text-violet-300 uppercase">
              DOCUMENT → CASES → REPORT
            </span>
          </div>
          <p className="text-xs text-slate-400 font-sans">
            {selectedProject
              ? `Contract tests for the API of ${selectedProject.name}, made from its OpenAPI document by rule, with no AI: what the document promises against what the API does`
              : 'Contract tests made from an API’s OpenAPI document by rule, with no AI: what the document promises against what the API does'}
          </p>
        </div>
      </header>

      <div aria-live="polite">{renderBody()}</div>
    </div>
  );
};

