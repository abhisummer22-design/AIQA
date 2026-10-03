/**
 * Phase 8C — the report of a regression run: how it came out, and for each case what it came to now beside what it came to the last time,
 * regressions first; what could not be repeated is listed with why.
 */
import React from 'react';
import { AlertTriangle, ClipboardCopy, Download, Loader2 } from 'lucide-react';
import type { RegressionCase, RegressionReport } from '../../types/contract';
import { formatTimestamp } from '../autopilot/autopilotModel';
import { CHANGE_LABEL, OUTCOME_LABEL, VERDICT_LABEL, changeLine, verdictTone, type Tone } from './regressionModel';

const BADGE: Record<Tone, string> = {
  good: 'bg-emerald-950/60 border-emerald-800 text-emerald-300',
  bad: 'bg-rose-950/60 border-rose-800 text-rose-300',
  warn: 'bg-amber-950/60 border-amber-800 text-amber-300',
  plain: 'bg-slate-900 border-slate-700 text-slate-400',
};

export const Badge: React.FC<{ tone: Tone; children: React.ReactNode } & React.HTMLAttributes<HTMLSpanElement>> = ({ tone, children, ...rest }) => (
  <span {...rest} className={`px-1.5 py-0.5 rounded border text-[10px] font-mono uppercase whitespace-nowrap ${BADGE[tone]}`}>
    {children}
  </span>
);

const changeTone = (c: RegressionCase): Tone =>
  c.change === 'REGRESSED' || c.change === 'STILL_FAILING' ? 'bad' : c.change === 'FIXED' ? 'good' : c.change === 'NEW' && c.outcome !== 'PASSED' && c.outcome !== 'PENDING' ? 'warn' : 'plain';

const TILE: Record<Tone, string> = {
  bad: 'bg-rose-950/30 border-rose-800/60',
  good: 'bg-emerald-950/20 border-emerald-800/50',
  warn: 'bg-amber-950/30 border-amber-800/60',
  plain: 'bg-[#020617] border-slate-800',
};

const Tile: React.FC<{ id: string; label: string; value: string; tone?: Tone; note?: string }> = ({ id, label, value, tone = 'plain', note }) => (
  <div id={id} className={`p-3 rounded-lg border ${TILE[tone]}`}>
    <div className="text-[10px] uppercase tracking-wide text-content-muted">{label}</div>
    <div className="text-sm font-mono text-slate-100">{value}</div>
    {note && <div className="text-[10px] text-content-muted mt-0.5">{note}</div>}
  </div>
);

const CaseRow: React.FC<{ c: RegressionCase }> = ({ c }) => (
  <li id={`regression-case-${c.case_id}`} data-case={c.case_id} data-change={c.change} data-outcome={c.outcome} className="p-2 rounded border border-slate-800/80 bg-[#020617] space-y-0.5">
    <div className="flex flex-wrap items-center gap-2 text-[11px]">
      <Badge tone={changeTone(c)}>{CHANGE_LABEL[c.change]}</Badge>
      <span className="text-slate-200">{c.title}</span>
      <span className="ml-auto text-content-muted">{OUTCOME_LABEL[c.outcome]}</span>
    </div>
    <div className="text-[11px] text-slate-400">{changeLine(c)}</div>
    {c.reason && (
      <div data-reason className="text-[11px] text-rose-300 break-words">
        {c.reason}
      </div>
    )}
  </li>
);

export type CopyState = 'idle' | 'copied' | 'shown';

export const RegressionReportView: React.FC<{
  report: RegressionReport;
  onCopyMarkdown: () => void;
  copyState: CopyState;
  markdown: string;
  /** Phase 8D: saves the report as JUnit XML (what a CI system reads). The button is there only when this is given. */
  onDownloadJunit?: () => void;
}> = ({ report, onCopyMarkdown, copyState, markdown, onDownloadJunit }) => {
  const s = report.summary;
  return (
    <section id="regression-report" className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-sm font-semibold text-slate-100">{report.schedule_name}</h2>
        <span id="regression-report-time" className="text-[11px] text-content-muted">
          {formatTimestamp(report.created_at)} · {report.trigger === 'SCHEDULE' ? 'by the schedule' : 'run by hand'}
        </span>
        <Badge id="regression-verdict" data-verdict={report.verdict} tone={verdictTone(report.verdict)}>
          {report.verdict === 'RUNNING' && <Loader2 className="w-3 h-3 animate-spin inline mr-1" />}
          {VERDICT_LABEL[report.verdict]}
        </Badge>
        <button
          type="button"
          id="regression-copy"
          onClick={onCopyMarkdown}
          className="ml-auto inline-flex items-center gap-1 px-2 py-1 rounded border border-slate-700 text-[11px] text-slate-200 hover:bg-slate-800 cursor-pointer"
        >
          <ClipboardCopy className="w-3 h-3" />
          {copyState === 'copied' ? 'Copied' : 'Copy as Markdown'}
        </button>
        {onDownloadJunit && (
          <button
            type="button"
            id="regression-junit"
            onClick={onDownloadJunit}
            title="The report as JUnit XML, which CI systems show as a test page"
            className="inline-flex items-center gap-1 px-2 py-1 rounded border border-slate-700 text-[11px] text-slate-200 hover:bg-slate-800 cursor-pointer"
          >
            <Download className="w-3 h-3" />
            Download JUnit
          </button>
        )}
      </div>
      {copyState === 'shown' && <textarea id="regression-markdown" aria-label="Regression report in markdown" readOnly value={markdown} rows={8} className="w-full rounded border border-slate-800 bg-[#020617] p-2 text-[11px] font-mono text-slate-300" />}

      <div className="grid grid-cols-2 md:grid-cols-6 gap-2">
        <Tile id="regression-tile-cases" label="Cases run" value={`${s.ran} of ${s.cases}`} note={s.pending > 0 ? `${s.pending} still waiting` : undefined} />
        <Tile id="regression-tile-passed" label="Passed" value={String(s.passed)} tone={s.ran > 0 && s.failed + s.error === 0 ? 'good' : 'plain'} />
        <Tile id="regression-tile-failed" label="Failed" value={String(s.failed + s.error)} tone={s.failed + s.error > 0 ? 'bad' : 'plain'} />
        <Tile id="regression-tile-regressed" label="Regressed" value={String(s.regressed)} tone={s.regressed > 0 ? 'bad' : 'plain'} note="passed last time, not now" />
        <Tile id="regression-tile-fixed" label="Fixed" value={String(s.fixed)} tone={s.fixed > 0 ? 'good' : 'plain'} />
        <Tile id="regression-tile-still-failing" label="Still failing" value={String(s.still_failing)} tone={s.still_failing > 0 ? 'bad' : 'plain'} />
      </div>

      {report.notes.length > 0 && (
        <ul id="regression-notes" className="space-y-1">
          {report.notes.map((n) => (
            <li key={n} className="flex items-start gap-1.5 text-[11px] text-amber-300">
              <AlertTriangle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
              <span>{n}</span>
            </li>
          ))}
        </ul>
      )}

      {report.cases.length > 0 ? (
        <ul id="regression-cases" className="space-y-1.5">
          {report.cases.map((c) => (
            <CaseRow key={c.case_id} c={c} />
          ))}
        </ul>
      ) : (
        <p id="regression-no-cases" className="text-[11px] text-content-muted">
          No case was repeated in this run.
        </p>
      )}

      {report.skipped.length > 0 && (
        <details id="regression-skipped" className="rounded-lg bg-[#0B1120] border border-slate-800">
          <summary className="p-2 cursor-pointer text-[11px] text-slate-300">Not repeated ({report.skipped.length})</summary>
          <ul className="px-3 pb-2 space-y-0.5 text-[11px] text-slate-400">
            {report.skipped.map((c) => (
              <li key={c.case_id}>
                <span className="text-slate-200">{c.title}</span>: {c.reason}
              </li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
};
