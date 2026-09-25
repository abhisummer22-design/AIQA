/**
 * Phase 7 — the approval gate: the run has written its cases and validated them, and waits for a person to look at them.
 *
 * A generated case is not trusted unseen: each is approved (or rejected) here by name, or all that passed validation are approved
 * at once, and then the run is let go on. What is left undecided is not run. Approving goes through
 * `POST .../autopilot/runs/:id/approve-cases`, so each approval is an ordinary one, audited under the person's name.
 */
import React, { useCallback, useEffect, useState } from 'react';
import { AlertTriangle, Check, CheckCheck, Loader2, PlayCircle, RefreshCw, X } from 'lucide-react';
import { aiqaApi } from '../../services/aiqaApi';
import type { AutopilotApproveCasesResponse, AutopilotRunDetail, TestCaseRow } from '../../types/contract';
import { errorText, plural, tallyCases, techniqueOf } from './autopilotModel';

const STATUS_STYLE: Record<string, string> = {
  GENERATED: 'text-amber-300 bg-amber-950/40 border-amber-800/60',
  VALIDATED: 'text-sky-300 bg-sky-950/40 border-sky-800/60',
  APPROVED: 'text-emerald-300 bg-emerald-950/40 border-emerald-800/60',
  REJECTED: 'text-rose-300 bg-rose-950/40 border-rose-800/60',
  EXECUTED: 'text-violet-300 bg-violet-950/40 border-violet-800/60',
};
const STATUS_WORD: Record<string, string> = {
  GENERATED: 'Needs fixing',
  VALIDATED: 'Validated',
  APPROVED: 'Approved',
  REJECTED: 'Rejected',
  EXECUTED: 'Run',
};

interface Props {
  projectId: string;
  runId: string;
  /** The suite the run's cases went into (`null`: it has none yet). */
  suiteId: string | null;
  onDetail: (detail: AutopilotRunDetail) => void;
}

export const GatePanel: React.FC<Props> = ({ projectId, runId, suiteId, onDetail }) => {
  const [cases, setCases] = useState<TestCaseRow[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [result, setResult] = useState<AutopilotApproveCasesResponse | null>(null);
  const [open, setOpen] = useState<Set<string>>(new Set());

  const load = useCallback(
    async (signal?: AbortSignal) => {
      if (!suiteId) {
        setCases([]);
        return;
      }
      try {
        const rows = await aiqaApi.testCases.list(projectId, { suiteId }, signal);
        if (signal?.aborted) return;
        setCases(rows);
        setLoadError(null);
      } catch (e) {
        if (signal?.aborted) return;
        setLoadError(errorText(e, 'The cases could not be read.'));
      }
    },
    [projectId, suiteId],
  );

  useEffect(() => {
    const controller = new AbortController();
    void load(controller.signal);
    return () => controller.abort();
  }, [load]);

  const guarded = async (what: string, action: () => Promise<void>) => {
    setBusy(what);
    setActionError(null);
    try {
      await action();
    } catch (e) {
      setActionError(errorText(e, 'That did not work.'));
    } finally {
      setBusy(null);
    }
  };

  const approve = (ids?: string[]) =>
    guarded(ids ? ids[0] : 'all', async () => {
      setResult(await aiqaApi.autopilot.runs.approveCases(projectId, runId, ids));
      await load();
    });
  const reject = (id: string) =>
    guarded(id, async () => {
      await aiqaApi.testCases.reject(projectId, id);
      setResult(null);
      await load();
    });
  const goOn = () =>
    guarded('continue', async () => {
      onDetail(await aiqaApi.autopilot.runs.continue(projectId, runId));
    });

  const toggle = (id: string) =>
    setOpen((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const rows = cases ?? [];
  const tally = tallyCases(rows);
  const undecided = tally.undecided;

  return (
    <section id="autopilot-gate" className="p-4 rounded-lg bg-[#0B1120] border border-amber-800/50 shadow-sm space-y-3">
      <div>
        <h2 className="text-xs font-semibold text-amber-200">The run is waiting for you to approve its cases</h2>
        <p className="text-[11px] text-slate-400">
          Read the cases, approve the ones to run and reject the ones that are wrong. The run runs only the approved cases; what is left undecided is not
          run.
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-2 text-[11px]">
        <span id="gate-tally" className="text-slate-300">
          {rows.length} {plural(rows.length, 'case', 'cases')}: {tally.approved} approved, {tally.validated} validated and waiting,{' '}
          {rows.filter((c) => c.status === 'GENERATED').length} needing a fix, {tally.rejected} rejected
        </span>
        <button
          type="button"
          id="gate-refresh"
          onClick={() => void load()}
          className="inline-flex items-center gap-1 px-2 py-0.5 rounded border border-slate-700 text-slate-300 hover:bg-slate-800 cursor-pointer"
        >
          <RefreshCw className="w-3 h-3" /> Refresh
        </button>
      </div>

      {cases === null && !loadError && (
        <p className="flex items-center gap-1.5 text-[11px] text-slate-400">
          <Loader2 className="w-3.5 h-3.5 animate-spin" /> Reading the cases…
        </p>
      )}
      {loadError && (
        <p id="gate-load-error" role="alert" className="text-[11px] text-rose-300">
          {loadError}
        </p>
      )}
      {cases !== null && rows.length === 0 && !loadError && (
        <p id="gate-empty" className="text-[11px] text-slate-400">
          The run has no cases: nothing was written for the requirements it read.
        </p>
      )}

      {rows.length > 0 && (
        <ul id="gate-cases" className="space-y-1.5 max-h-[28rem] overflow-y-auto pr-1">
          {rows.map((c) => {
            const technique = techniqueOf(c.tags);
            const isOpen = open.has(c.id);
            return (
              <li key={c.id} id={`gate-case-${c.id}`} className="p-2 rounded bg-[#020617] border border-slate-800 space-y-1">
                <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                  <button
                    type="button"
                    onClick={() => toggle(c.id)}
                    aria-expanded={isOpen}
                    className="text-left text-xs text-slate-100 hover:text-violet-200 cursor-pointer"
                  >
                    {c.title}
                  </button>
                  <span className={`px-1.5 py-0.5 rounded border text-[10px] font-mono ${STATUS_STYLE[c.status] ?? STATUS_STYLE.GENERATED}`}>
                    {STATUS_WORD[c.status] ?? c.status}
                  </span>
                  {c.requirement_reference && <span className="font-mono text-[10px] text-slate-400">{c.requirement_reference}</span>}
                  {technique && <span className="font-mono text-[10px] text-slate-500">{technique}</span>}
                  <span className="ml-auto flex gap-1">
                    {c.status === 'VALIDATED' && (
                      <button
                        type="button"
                        id={`gate-approve-${c.id}`}
                        onClick={() => void approve([c.id])}
                        disabled={busy !== null}
                        className="inline-flex items-center gap-1 px-2 py-0.5 rounded border border-emerald-700 text-emerald-300 hover:bg-emerald-950/40 text-[11px] cursor-pointer disabled:opacity-50"
                      >
                        <Check className="w-3 h-3" /> Approve
                      </button>
                    )}
                    {/* The backend rejects a case only before it is approved (an approved one may already be in a handoff). */}
                    {(c.status === 'VALIDATED' || c.status === 'GENERATED') && (
                      <button
                        type="button"
                        id={`gate-reject-${c.id}`}
                        onClick={() => void reject(c.id)}
                        disabled={busy !== null}
                        className="inline-flex items-center gap-1 px-2 py-0.5 rounded border border-rose-800 text-rose-300 hover:bg-rose-950/40 text-[11px] cursor-pointer disabled:opacity-50"
                      >
                        <X className="w-3 h-3" /> Reject
                      </button>
                    )}
                  </span>
                </div>
                {c.status === 'GENERATED' && c.validation_notes && <p className="text-[11px] text-amber-300">{c.validation_notes}</p>}
                {isOpen && (
                  <div className="text-[11px] text-slate-400 space-y-1 pl-1">
                    {c.description && <p>{c.description}</p>}
                    {c.preconditions.length > 0 && <p>Before: {c.preconditions.join('; ')}</p>}
                    <ol className="list-decimal pl-5 space-y-0.5">
                      {c.steps.map((s) => (
                        <li key={s.step_number}>
                          {s.action} <span className="text-slate-500">→ {s.expected_result}</span>
                        </li>
                      ))}
                    </ol>
                    <p>Expected: {c.expected_result}</p>
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}

      {result && (
        <div id="gate-result" className="p-2 rounded bg-[#020617] border border-slate-800 text-[11px] space-y-1">
          <p className="text-emerald-300">
            {result.approved.length} {plural(result.approved.length, 'case', 'cases')} approved
            {result.refused.length > 0 ? `, ${result.refused.length} refused:` : '.'}
          </p>
          {result.refused.length > 0 && (
            <ul className="space-y-0.5">
              {result.refused.map((r) => (
                <li key={r.id} className="flex items-start gap-1.5 text-amber-300">
                  <AlertTriangle className="w-3 h-3 mt-0.5 shrink-0" />
                  <span>
                    {r.title}: {r.reason}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}

      {actionError && (
        <p id="gate-error" role="alert" className="text-[11px] text-rose-300">
          {actionError}
        </p>
      )}

      <div className="flex flex-wrap items-center gap-2 pt-1">
        <button
          type="button"
          id="gate-approve-all"
          onClick={() => void approve()}
          disabled={busy !== null || tally.validated === 0}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md border border-emerald-700 text-emerald-200 hover:bg-emerald-950/40 text-xs cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {busy === 'all' ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <CheckCheck className="w-3.5 h-3.5" />}
          Approve all {tally.validated} validated
        </button>
        <button
          type="button"
          id="gate-continue"
          onClick={() => void goOn()}
          disabled={busy !== null}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 hover:bg-violet-500 text-white text-xs font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {busy === 'continue' ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <PlayCircle className="w-3.5 h-3.5" />}
          Continue: run the {tally.approved} approved {plural(tally.approved, 'case', 'cases')}
        </button>
        {undecided > 0 && (
          <span id="gate-undecided" className="text-[11px] text-amber-300">
            {undecided} {plural(undecided, 'case is', 'cases are')} undecided and will not be run.
          </span>
        )}
        {tally.approved === 0 && (
          <span id="gate-none-approved" className="text-[11px] text-amber-300">
            No case is approved: continuing runs nothing.
          </span>
        )}
      </div>
    </section>
  );
};
