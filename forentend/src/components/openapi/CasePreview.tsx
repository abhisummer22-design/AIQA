/**
 * Phase 8B — the cases the service would make from the document, before anything is run: the operations (each can be left out), and for
 * each the cases with the request and the answer each expects. Nothing has been sent to the API yet.
 */
import React from 'react';
import { AlertTriangle, Loader2, Lock, Play } from 'lucide-react';
import type { OpenApiCase, OpenApiPlan } from '../../types/contract';
import { plural } from '../autopilot/autopilotModel';
import { KIND_LABEL, expectText, pathOf } from './openApiModel';

const Tile: React.FC<{ id: string; label: string; value: string; note?: string; amber?: boolean }> = ({ id, label, value, note, amber = false }) => (
  <div id={id} className={`p-3 rounded-lg border ${amber ? 'bg-amber-950/30 border-amber-800/60' : 'bg-[#020617] border-slate-800'}`}>
    <div className="text-[10px] uppercase tracking-wide text-content-muted">{label}</div>
    <div className={`text-sm font-mono ${amber ? 'text-amber-200' : 'text-slate-100'}`}>{value}</div>
    {note && <div className="text-[10px] text-content-muted mt-0.5">{note}</div>}
  </div>
);

/** A case's title without the operation it starts with (`GET /pets: without …` is shown under `GET /pets`). */
export const titleWithin = (c: Pick<OpenApiCase, 'title' | 'operation'>): string => (c.title.startsWith(`${c.operation}: `) ? c.title.slice(c.operation.length + 2) : c.title);

export const CasePreview: React.FC<{
  plan: OpenApiPlan;
  selected: ReadonlySet<string>;
  onToggle: (operation: string) => void;
  onSelectAll: (all: boolean) => void;
  onRun: () => void;
  running: boolean;
  unsafeAllowed: boolean;
}> = ({ plan, selected, onToggle, onSelectAll, onRun, running, unsafeAllowed }) => {
  const toRun = plan.cases.filter((c) => selected.has(c.operation) && c.skipped === null).length;
  const missing = plan.missing_secrets;
  const blockedReason = missing.length > 0 ? 'A variable the credentials name is not set.' : toRun === 0 ? 'Choose an operation that has a case to run.' : null;
  return (
    <section id="openapi-preview" className="space-y-3">
      <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
        <Tile id="openapi-tile-operations" label="Operations" value={String(plan.operations.length)} note={`${plan.spec.title} ${plan.spec.version}`} />
        <Tile id="openapi-tile-cases" label="Cases" value={String(plan.counts.cases)} />
        <Tile id="openapi-tile-run" label="To run" value={String(toRun)} note={`against ${plan.base_url}`} />
        <Tile
          id="openapi-tile-not-run"
          label="Not run"
          value={String(plan.counts.not_run)}
          note={plan.counts.not_run > 0 && !unsafeAllowed ? 'they change data: allow that above to run them' : undefined}
          amber={plan.counts.not_run > 0}
        />
      </div>

      {missing.length > 0 && (
        <p id="openapi-missing-secrets" role="alert" className="flex items-start gap-1.5 p-2 rounded bg-amber-950/30 border border-amber-800/60 text-[11px] text-amber-200">
          <Lock className="w-3.5 h-3.5 mt-0.5 shrink-0" />
          <span>
            {plural(missing.length, 'This variable is', 'These variables are')} not set in the orchestrator’s environment: <span className="font-mono">{missing.join(', ')}</span>. Add{' '}
            {plural(missing.length, 'it', 'them')} to its .env and restart it; the run cannot start until then.
          </span>
        </p>
      )}
      {plan.notes.length > 0 && (
        <ul id="openapi-notes" className="space-y-1">
          {plan.notes.map((n) => (
            <li key={n} className="flex items-start gap-1.5 text-[11px] text-amber-300">
              <AlertTriangle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
              <span>{n}</span>
            </li>
          ))}
        </ul>
      )}

      <div className="flex items-center gap-3 text-[11px] text-slate-400">
        <span>
          {selected.size} of {plan.operations.length} {plural(plan.operations.length, 'operation', 'operations')} chosen
        </span>
        <button type="button" id="openapi-select-all" onClick={() => onSelectAll(true)} className="text-violet-300 hover:underline cursor-pointer">
          all
        </button>
        <button type="button" id="openapi-select-none" onClick={() => onSelectAll(false)} className="text-violet-300 hover:underline cursor-pointer">
          none
        </button>
      </div>

      <div className="space-y-2">
        {plan.operations.map((op) => {
          const cases = plan.cases.filter((c) => c.operation === op.id);
          const why = plan.skipped.find((s) => s.operation === op.id)?.reason;
          return (
            <details key={op.id} id={`openapi-op-${op.id}`} className="rounded-lg bg-[#0B1120] border border-slate-800" open={cases.length > 0 && cases.length <= 6}>
              <summary className="flex flex-wrap items-center gap-2 p-2 cursor-pointer text-[11px]">
                <input
                  type="checkbox"
                  aria-label={`Test ${op.id}`}
                  checked={selected.has(op.id)}
                  onClick={(e) => e.stopPropagation()}
                  onChange={() => onToggle(op.id)}
                  className="accent-violet-500"
                />
                <span className="font-mono text-slate-100">{op.id}</span>
                {op.summary && <span className="text-slate-400">{op.summary}</span>}
                {op.requires_auth && (
                  <span title="the document says this operation needs a credential" className="inline-flex items-center gap-0.5 text-content-muted">
                    <Lock className="w-3 h-3" /> credential
                  </span>
                )}
                {op.deprecated && <span className="px-1 rounded border border-slate-700 text-[10px] text-slate-400">deprecated</span>}
                <span className="ml-auto text-content-muted">
                  {cases.length} {plural(cases.length, 'case', 'cases')}
                </span>
              </summary>
              {cases.length === 0 ? (
                <p className="px-3 pb-2 text-[11px] text-content-muted">No case: {why ?? 'none of the asked-for kinds applies to it'}.</p>
              ) : (
                <div className="px-3 pb-2 overflow-x-auto">
                  <table className="w-full text-[11px] text-left">
                    <caption className="sr-only">Cases for {op.id}</caption>
                    <thead className="text-content-muted">
                      <tr>
                        <th scope="col" className="py-1 pr-3 font-normal">Kind</th>
                        <th scope="col" className="py-1 pr-3 font-normal">Case</th>
                        <th scope="col" className="py-1 pr-3 font-normal">Request</th>
                        <th scope="col" className="py-1 font-normal">Expects</th>
                      </tr>
                    </thead>
                    <tbody>
                      {cases.map((c) => (
                        <tr key={c.id} data-case={c.id} className="border-t border-slate-800/70 align-top">
                          <td className="py-1 pr-3 whitespace-nowrap text-slate-300">{KIND_LABEL[c.kind].title}</td>
                          <td className="py-1 pr-3 text-slate-200">
                            {titleWithin(c)}
                            {c.skipped && (
                              <span title={c.skipped} className="ml-1.5 px-1 rounded border border-amber-800/60 text-[10px] text-amber-300">
                                not run: changes data
                              </span>
                            )}
                          </td>
                          <td className="py-1 pr-3 font-mono text-slate-400 break-all">
                            {c.request.method} {pathOf(c.request.url)}
                          </td>
                          <td className="py-1 font-mono text-slate-300 whitespace-nowrap">{expectText({ ...c.expect, schema: c.expect.schema !== null && c.expect.schema !== undefined })}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </details>
          );
        })}
      </div>

      <div className="flex flex-wrap items-center gap-3">
        <button
          type="button"
          id="openapi-run"
          onClick={onRun}
          disabled={blockedReason !== null || running}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-md bg-violet-600 text-xs font-semibold text-white hover:bg-violet-500 cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {running ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <Play className="w-3.5 h-3.5" />}
          {running ? 'Starting…' : `Run ${toRun} ${plural(toRun, 'case', 'cases')}`}
        </button>
        {blockedReason && (
          <span id="openapi-run-blocked" className="text-[11px] text-content-muted">
            {blockedReason}
          </span>
        )}
      </div>
    </section>
  );
};
