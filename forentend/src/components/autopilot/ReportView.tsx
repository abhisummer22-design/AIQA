/**
 * Phase 7 — the report of an autopilot run (`autopilot_report.rs`): the requirements one by one with how their cases came out,
 * the failures, the cases that could not give an answer, what the AI wrote and cost, and what to know before trusting the numbers.
 *
 * A report is loaded when a run has ended, or on request for one that is still going (then it says it is not final).
 */
import React, { useCallback, useEffect, useMemo, useState } from 'react';
import { AlertTriangle, ClipboardCopy, Loader2 } from 'lucide-react';
import { aiqaApi } from '../../services/aiqaApi';
import type { AutopilotReport, AutopilotReportCase, AutopilotVerdict } from '../../types/contract';
import {
  STEP_LABEL,
  VERDICT_LABEL,
  VERDICT_ORDER,
  coverageLine,
  errorText,
  formatCost,
  formatDurationRange,
  formatRange,
  formatSeconds,
  formatTimestamp,
  outcomeLabel,
  plural,
  providerLabel,
  reportTone,
  withProviderNames,
  type Tone,
} from './autopilotModel';

const TONE_BOX: Record<Tone, string> = {
  good: 'bg-emerald-950/30 border-emerald-800/60 text-emerald-200',
  warn: 'bg-amber-950/30 border-amber-800/60 text-amber-200',
  bad: 'bg-rose-950/30 border-rose-800/60 text-rose-200',
  plain: 'bg-[#020617] border-slate-800 text-slate-200',
};
const VERDICT_STYLE: Record<AutopilotVerdict, string> = {
  PASSED: 'text-emerald-300 bg-emerald-950/40 border-emerald-800/60',
  FAILED: 'text-rose-300 bg-rose-950/40 border-rose-800/60',
  INCONCLUSIVE: 'text-amber-300 bg-amber-950/40 border-amber-800/60',
  NOT_EXECUTED: 'text-slate-300 bg-slate-800/60 border-slate-700',
  NO_CASES: 'text-slate-400 bg-slate-900 border-slate-700',
};

const Tile: React.FC<{ id: string; label: string; value: string }> = ({ id, label, value }) => (
  <div id={id} className="p-2.5 rounded-md bg-[#020617] border border-slate-800">
    <div className="text-sm font-mono font-semibold text-slate-100">{value}</div>
    <div className="text-[10px] text-slate-500 leading-tight">{label}</div>
  </div>
);

const CaseList: React.FC<{ id: string; title: string; cases: AutopilotReportCase[] }> = ({ id, title, cases }) => {
  if (cases.length === 0) return null;
  return (
    <div id={id} className="space-y-1.5">
      <h3 className="text-[11px] font-semibold text-slate-300">
        {title} ({cases.length})
      </h3>
      <ul className="space-y-1.5">
        {cases.map((c) => (
          <li key={c.case_id} className="p-2 rounded bg-[#020617] border border-slate-800 space-y-0.5">
            <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
              <span className="text-xs text-slate-100">{c.title}</span>
              {c.requirement && <span className="font-mono text-[10px] text-slate-400">{c.requirement}</span>}
              <span className="font-mono text-[10px] text-slate-300">{outcomeLabel(c.outcome)}</span>
            </div>
            {c.reason && <p className="text-[11px] text-slate-400 break-words">{c.reason}</p>}
            {c.attempts.length > 1 && (
              <ul className="text-[10px] text-slate-500 list-disc pl-5">
                {c.attempts.map((a) => (
                  <li key={a.run_id}>
                    {outcomeLabel(a.outcome)}
                    {a.reason ? `: ${a.reason}` : ''}
                  </li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
};

interface Props {
  projectId: string;
  runId: string;
  /** Loaded when the component is shown; a run that is still going asks for it with a button instead. */
  autoLoad: boolean;
  /** Changes when the run's state or steps do, so a report shown for a running run is fetched again. */
  version: string;
  /** The name a provider was given (Settings) for its id; the id itself when it is not known. */
  providerName?: (id: string) => string;
}

const AS_IS = (id: string) => id;

export const ReportView: React.FC<Props> = ({ projectId, runId, autoLoad, version, providerName = AS_IS }) => {
  const [wanted, setWanted] = useState(autoLoad);
  const [report, setReport] = useState<AutopilotReport | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [filter, setFilter] = useState<'ALL' | AutopilotVerdict>('ALL');
  const [copy, setCopy] = useState<{ state: 'idle' | 'copied' | 'shown'; text: string }>({ state: 'idle', text: '' });

  useEffect(() => setWanted(autoLoad), [autoLoad, runId]);

  useEffect(() => {
    if (!wanted) return;
    const controller = new AbortController();
    setLoading(true);
    aiqaApi.autopilot.runs
      .report(projectId, runId, controller.signal)
      .then((r) => {
        setReport(r);
        setError(null);
      })
      .catch((e) => {
        if (!controller.signal.aborted) setError(errorText(e, 'The report could not be read.'));
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [wanted, projectId, runId, version]);

  const copyMarkdown = useCallback(async () => {
    try {
      const text = await aiqaApi.autopilot.runs.reportMarkdown(projectId, runId);
      try {
        await navigator.clipboard.writeText(text);
        setCopy({ state: 'copied', text });
      } catch {
        // No clipboard (an insecure page, a denied permission): show the text to copy by hand.
        setCopy({ state: 'shown', text });
      }
    } catch (e) {
      setError(errorText(e, 'The report could not be read as text.'));
    }
  }, [projectId, runId]);

  const rows = useMemo(() => {
    const order = new Map(VERDICT_ORDER.map((v, i) => [v, i]));
    return [...(report?.coverage ?? [])]
      .filter((r) => filter === 'ALL' || r.verdict === filter)
      .sort((a, b) => (order.get(a.verdict) ?? 99) - (order.get(b.verdict) ?? 99));
  }, [report, filter]);

  if (!wanted) {
    return (
      <section id="autopilot-report" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800 space-y-2">
        <h2 className="text-xs font-semibold text-slate-100">Report</h2>
        <button
          type="button"
          id="report-preview"
          onClick={() => setWanted(true)}
          className="px-3 py-1.5 rounded-md border border-slate-700 text-xs text-slate-200 hover:bg-slate-800 cursor-pointer"
        >
          Show what the run has found so far
        </button>
      </section>
    );
  }

  if (!report) {
    return (
      <section id="autopilot-report" className="p-4 rounded-lg bg-[#0B1120] border border-slate-800">
        {error ? (
          <p role="alert" className="text-[11px] text-rose-300">
            {error}
          </p>
        ) : (
          <p className="flex items-center gap-1.5 text-[11px] text-slate-400">
            <Loader2 className="w-3.5 h-3.5 animate-spin" /> Working out the report…
          </p>
        )}
      </section>
    );
  }

  const tone = reportTone(report.coverage_summary);
  const seconds = report.run.seconds;
  const { execution, generation, cost } = report;
  const writers: Array<[string, number]> = Object.entries<number>(generation.providers ?? {}).map(([key, groups]) => [providerLabel(key, providerName), groups]);

  return (
    <section id="autopilot-report" className={`p-4 rounded-lg bg-[#0B1120] border border-slate-800 shadow-sm space-y-3 ${loading ? 'opacity-70' : ''}`} aria-busy={loading}>
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-xs font-semibold text-slate-100">Report</h2>
        {!report.final && (
          <span id="report-not-final" className="px-1.5 py-0.5 rounded border border-amber-800/60 bg-amber-950/40 text-[10px] font-mono text-amber-300">
            not final: the run has not ended
          </span>
        )}
        <button
          type="button"
          id="report-copy"
          onClick={() => void copyMarkdown()}
          className="ml-auto inline-flex items-center gap-1 px-2 py-1 rounded border border-slate-700 text-[11px] text-slate-200 hover:bg-slate-800 cursor-pointer"
        >
          <ClipboardCopy className="w-3 h-3" /> {copy.state === 'copied' ? 'Copied' : 'Copy as Markdown'}
        </button>
      </div>
      {copy.state === 'shown' && (
        <textarea id="report-markdown" readOnly value={copy.text} rows={8} className="w-full rounded border border-slate-800 bg-[#020617] p-2 text-[11px] font-mono text-slate-300" />
      )}
      {error && (
        <p role="alert" className="text-[11px] text-rose-300">
          {error}
        </p>
      )}

      <div id="report-headline" className={`p-3 rounded-lg border text-sm ${TONE_BOX[tone]}`}>
        {coverageLine(report.coverage_summary)}
        <div className="text-[11px] opacity-80 mt-0.5">
          {report.run.name} · {report.run.state.toLowerCase().replace('_', ' ')}
          {seconds !== null ? ` · took ${formatSeconds(seconds)}` : ''}
          {report.run.finished_at ? ` · ended ${formatTimestamp(report.run.finished_at)}` : ''}
        </div>
      </div>

      <div className="grid grid-cols-2 sm:grid-cols-5 gap-2">
        <Tile id="report-tile-requirements" label="requirements" value={String(report.requirements.total)} />
        <Tile id="report-tile-cases" label="cases written" value={generation.cases === null ? '—' : String(generation.cases)} />
        <Tile
          id="report-tile-run"
          label="cases run"
          value={execution.enabled ? `${execution.finished ?? 0} of ${execution.cases ?? 0}` : 'not asked for'}
        />
        <Tile id="report-tile-passed" label="passed" value={String(execution.passed)} />
        <Tile id="report-tile-failed" label="failed" value={String(execution.failed)} />
      </div>

      {report.notes.length > 0 && (
        <ul id="report-notes" className="space-y-1">
          {report.notes.map((n) => (
            <li key={n} className="flex items-start gap-1.5 p-2 rounded bg-amber-950/30 border border-amber-800/60 text-[11px] text-amber-300">
              <AlertTriangle className="w-3 h-3 shrink-0 mt-0.5" />
              <span>{withProviderNames(n, providerName)}</span>
            </li>
          ))}
        </ul>
      )}

      <CaseList id="report-findings" title="Cases that failed" cases={report.findings} />
      <CaseList id="report-not-run" title="Cases that could not give an answer" cases={report.not_run} />

      <div id="report-coverage" className="space-y-1.5">
        <div className="flex flex-wrap items-center gap-1.5">
          <h3 className="text-[11px] font-semibold text-slate-300 mr-1">Requirements</h3>
          {(['ALL', ...VERDICT_ORDER] as const).map((v) => {
            const count = v === 'ALL' ? report.coverage.length : (report.coverage_summary[v] ?? 0);
            if (v !== 'ALL' && count === 0) return null;
            return (
              <button
                key={v}
                type="button"
                id={`report-filter-${v}`}
                aria-pressed={filter === v}
                onClick={() => setFilter(v)}
                className={`px-2 py-0.5 rounded border text-[11px] font-mono cursor-pointer ${filter === v ? 'bg-violet-950/60 border-violet-600 text-violet-200' : 'bg-[#020617] border-slate-800 text-slate-300 hover:bg-slate-900'}`}
              >
                {v === 'ALL' ? 'All' : VERDICT_LABEL[v]} {count}
              </button>
            );
          })}
        </div>
        {rows.length === 0 ? (
          <p className="text-[11px] text-slate-500">No requirement to show.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-[11px] text-left">
              <thead className="text-slate-500">
                <tr>
                  <th className="py-1 pr-3 font-normal">Requirement</th>
                  <th className="py-1 pr-3 font-normal">Cases</th>
                  <th className="py-1 pr-3 font-normal">Ran</th>
                  <th className="py-1 pr-3 font-normal">Passed</th>
                  <th className="py-1 pr-3 font-normal">Failed</th>
                  <th className="py-1 font-normal">Verdict</th>
                </tr>
              </thead>
              <tbody id="report-coverage-rows">
                {rows.map((r) => (
                  <tr key={r.key} id={`report-req-${r.key}`} className="border-t border-slate-800/70 align-top">
                    <td className="py-1 pr-3">
                      <span className="font-mono text-slate-200">{r.key}</span> <span className="text-slate-500">{r.kind}</span>
                      <div className="text-slate-400">{r.text}</div>
                    </td>
                    <td className="py-1 pr-3 font-mono text-slate-300">{r.cases}</td>
                    <td className="py-1 pr-3 font-mono text-slate-300">{r.ran}</td>
                    <td className="py-1 pr-3 font-mono text-slate-300">{r.passed}</td>
                    <td className="py-1 pr-3 font-mono text-slate-300">{r.failed}</td>
                    <td className="py-1">
                      <span className={`px-1.5 py-0.5 rounded border text-[10px] font-mono ${VERDICT_STYLE[r.verdict]}`}>{VERDICT_LABEL[r.verdict]}</span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <div id="report-cost" className="text-[11px] text-slate-400 space-y-0.5">
        <h3 className="text-[11px] font-semibold text-slate-300">What it cost</h3>
        {cost.estimated ? (
          <p>
            Estimated: {formatRange(cost.estimated.ai_calls)} AI {plural(cost.estimated.ai_calls.max, 'call', 'calls')}, {formatDurationRange(cost.estimated.seconds)},{' '}
            {formatCost(cost.estimated.cost_usd)}.
          </p>
        ) : (
          <p>No estimate was kept with this run.</p>
        )}
        <p>
          Measured: {cost.actual.ai_prompt_tokens ?? 0} tokens in, {cost.actual.ai_completion_tokens ?? 0} out
          {cost.actual.ai_seconds !== null ? `, ${formatSeconds(cost.actual.ai_seconds)} of AI time` : ''}
          {cost.actual.run_seconds !== null ? `, ${formatSeconds(cost.actual.run_seconds)} running the cases` : ''}.
        </p>
        {writers.length > 0 && (
          <p id="report-providers">
            Written by {writers.map(([name, groups]) => `${name} (${groups} ${plural(groups, 'group', 'groups')})`).join(', ')}
            {generation.fallback_used ? ' — a fallback provider took over for some groups' : ''}.
          </p>
        )}
      </div>

      <details id="report-steps" className="text-[11px] text-slate-400">
        <summary className="cursor-pointer text-slate-300">Steps</summary>
        <table className="mt-1 w-full text-left">
          <tbody>
            {report.steps.map((s) => (
              <tr key={s.name} className="border-t border-slate-800/70">
                <td className="py-0.5 pr-3 text-slate-200">{STEP_LABEL[s.name]?.title ?? s.name}</td>
                <td className="py-0.5 pr-3 font-mono">{s.state.toLowerCase()}</td>
                <td className="py-0.5 pr-3 font-mono">{s.attempts > 1 ? `${s.attempts} attempts` : ''}</td>
                <td className="py-0.5 font-mono">{s.seconds === null ? '' : formatSeconds(s.seconds)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </section>
  );
};
