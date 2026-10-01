/**
 * Phase 8B — the report of an OpenAPI test run, as `openapi_runs.rs` works it out: what each operation of the document came to, for each
 * case what was expected and what came back, and how much of the document was tested at all. While the run is going it is what has been
 * found so far, and says so.
 */
import React from 'react';
import { AlertTriangle, ClipboardCopy, Download, Loader2 } from 'lucide-react';
import type { OpenApiReport, OpenApiReportCase, OpenApiReportOperation, OpenApiVerdict } from '../../types/contract';
import { plural } from '../autopilot/autopilotModel';
import { titleWithin } from './CasePreview';
import { KIND_LABEL, OUTCOME_LABEL, VERDICT_LABEL, countsLine, expectText, outcomeTone, passRateText, pathOf, type Tone } from './openApiModel';

const BADGE: Record<Tone, string> = {
  good: 'bg-emerald-950/50 border-emerald-800/60 text-emerald-300',
  bad: 'bg-rose-950/50 border-rose-800/60 text-rose-300',
  warn: 'bg-amber-950/40 border-amber-800/60 text-amber-300',
  plain: 'bg-slate-900 border-slate-700 text-slate-400',
};

const Badge: React.FC<{ verdict: OpenApiVerdict; label?: string }> = ({ verdict, label }) => (
  <span data-verdict={verdict} className={`px-1.5 py-0.5 rounded border text-[10px] font-mono uppercase ${BADGE[outcomeTone(verdict)]}`}>
    {label ?? VERDICT_LABEL[verdict]}
  </span>
);

const Tile: React.FC<{ id: string; label: string; value: string; tone?: Tone; note?: string }> = ({ id, label, value, tone = 'plain', note }) => (
  <div
    id={id}
    className={`p-3 rounded-lg border ${tone === 'bad' ? 'bg-rose-950/30 border-rose-800/60' : tone === 'good' ? 'bg-emerald-950/20 border-emerald-800/50' : tone === 'warn' ? 'bg-amber-950/30 border-amber-800/60' : 'bg-[#020617] border-slate-800'}`}
  >
    <div className="text-[10px] uppercase tracking-wide text-content-muted">{label}</div>
    <div className="text-sm font-mono text-slate-100">{value}</div>
    {note && <div className="text-[10px] text-content-muted mt-0.5">{note}</div>}
  </div>
);

const CaseRow: React.FC<{ operation: string; c: OpenApiReportCase }> = ({ operation, c }) => {
  const expected = expectText(c.expect);
  return (
    <li data-case={c.id} data-outcome={c.outcome} className="p-2 rounded border border-slate-800/80 bg-[#020617] space-y-0.5">
      <div className="flex flex-wrap items-center gap-2 text-[11px]">
        <Badge verdict={c.outcome} label={OUTCOME_LABEL[c.outcome]} />
        <span className="text-content-muted">{KIND_LABEL[c.kind].title}</span>
        <span className="text-slate-200">{titleWithin({ title: c.title, operation })}</span>
      </div>
      <div className="text-[11px] font-mono text-slate-400 break-all">
        {c.request.method} {pathOf(c.request.url)}
      </div>
      {c.outcome !== 'NOT_RUN' && (
        <div className="text-[11px] text-slate-400">
          expected <span className="font-mono text-slate-300">{expected}</span>
          {c.actual && (
            <>
              {' · '}got <span className={`font-mono ${c.outcome === 'PASSED' ? 'text-slate-300' : 'text-rose-300'}`}>{c.actual.status}</span>
              {' in '}
              {c.actual.duration_ms} ms
            </>
          )}
        </div>
      )}
      {c.reason && (
        <div data-reason className={`text-[11px] ${c.outcome === 'NOT_RUN' ? 'text-content-muted' : 'text-rose-300'}`}>
          {c.reason}
        </div>
      )}
    </li>
  );
};

const OperationBlock: React.FC<{ op: OpenApiReportOperation }> = ({ op }) => {
  const failing = op.verdict === 'FAILED' || op.verdict === 'ERROR';
  return (
    <details id={`openapi-report-op-${op.id}`} data-verdict={op.verdict} open={failing} className="rounded-lg bg-[#0B1120] border border-slate-800">
      <summary className="flex flex-wrap items-center gap-2 p-2 cursor-pointer text-[11px]">
        <Badge verdict={op.verdict} />
        <span className="font-mono text-slate-100">{op.id}</span>
        {op.summary && <span className="text-slate-400">{op.summary}</span>}
        <span className="ml-auto text-content-muted">{op.counts ? countsLine(op.counts) : (op.why_no_cases ?? 'no cases')}</span>
      </summary>
      {op.cases.length > 0 ? (
        <ul className="px-2 pb-2 space-y-1.5">
          {op.cases.map((c) => (
            <CaseRow key={c.id} operation={op.id} c={c} />
          ))}
        </ul>
      ) : (
        <p className="px-3 pb-2 text-[11px] text-content-muted">No case was made: {op.why_no_cases ?? 'none applies to it'}.</p>
      )}
    </details>
  );
};

export const OpenApiReportView: React.FC<{
  report: OpenApiReport;
  onCopyMarkdown: () => void;
  copyState: 'idle' | 'copied' | 'shown';
  markdown: string;
  refreshing?: boolean;
  /** Phase 8D: saves the report as JUnit XML (what a CI system reads). The button is there only when this is given. */
  onDownloadJunit?: () => void;
}> = ({ report, onCopyMarkdown, copyState, markdown, refreshing = false, onDownloadJunit }) => {
  const s = report.summary;
  return (
    <section id="openapi-report" className="space-y-3">
      <div className="flex flex-wrap items-center gap-2">
        <h2 className="text-sm font-semibold text-slate-100">
          {report.spec.title} <span className="font-normal text-slate-400">{report.spec.version}</span>
        </h2>
        <span id="openapi-report-target" className="text-[11px] font-mono text-content-muted">
          {report.spec.base_url}
        </span>
        {!report.final ? (
          <span id="openapi-report-not-final" className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded border border-amber-800/60 bg-amber-950/40 text-[10px] font-mono text-amber-300">
            <Loader2 className="w-3 h-3 animate-spin" /> running
          </span>
        ) : (
          <span id="openapi-report-final" className="px-1.5 py-0.5 rounded border border-slate-700 text-[10px] font-mono text-slate-400">
            {report.run_state.toLowerCase().replace(/_/g, ' ')}
          </span>
        )}
        <button
          type="button"
          id="openapi-copy"
          onClick={onCopyMarkdown}
          className="ml-auto inline-flex items-center gap-1 px-2 py-1 rounded border border-slate-700 text-[11px] text-slate-200 hover:bg-slate-800 cursor-pointer"
        >
          <ClipboardCopy className="w-3 h-3" />
          {copyState === 'copied' ? 'Copied' : 'Copy as Markdown'}
        </button>
        {onDownloadJunit && (
          <button
            type="button"
            id="openapi-junit"
            onClick={onDownloadJunit}
            title="The report as JUnit XML, which CI systems show as a test page"
            className="inline-flex items-center gap-1 px-2 py-1 rounded border border-slate-700 text-[11px] text-slate-200 hover:bg-slate-800 cursor-pointer"
          >
            <Download className="w-3 h-3" />
            Download JUnit
          </button>
        )}
      </div>
      {copyState === 'shown' && <textarea id="openapi-markdown" readOnly value={markdown} rows={8} className="w-full rounded border border-slate-800 bg-[#020617] p-2 text-[11px] font-mono text-slate-300" />}

      <div className={`grid grid-cols-2 md:grid-cols-6 gap-2 ${refreshing ? 'opacity-80' : ''}`}>
        <Tile id="openapi-tile-ran" label="Cases run" value={`${s.ran} of ${s.cases}`} />
        <Tile id="openapi-tile-passed" label="Passed" value={String(s.passed)} tone={s.passed > 0 && s.failed === 0 && s.error === 0 ? 'good' : 'plain'} />
        <Tile id="openapi-tile-failed" label="Failed" value={String(s.failed)} tone={s.failed > 0 ? 'bad' : 'plain'} />
        <Tile id="openapi-tile-error" label="Could not run" value={String(s.error)} tone={s.error > 0 ? 'warn' : 'plain'} />
        <Tile id="openapi-tile-not-run" label="Not run" value={String(s.not_run + s.pending)} note={s.pending > 0 ? `${s.pending} still waiting` : undefined} />
        <Tile id="openapi-tile-rate" label="Pass rate" value={passRateText(s.pass_rate)} />
      </div>

      <p id="openapi-coverage" className="text-[11px] text-slate-300">
        {report.coverage.tested} of {report.coverage.operations} {plural(report.coverage.operations, 'operation', 'operations')} of the document tested.
        {report.coverage.not_tested.length > 0 && (
          <span className="text-content-muted">
            {' '}
            Not tested:{' '}
            {report.coverage.not_tested.map((n, i) => (
              <span key={n.operation} data-not-tested={n.operation}>
                {i > 0 && '; '}
                <span className="font-mono">{n.operation}</span> ({n.reason})
              </span>
            ))}
            .
          </span>
        )}
      </p>

      {report.notes.length > 0 && (
        <ul id="openapi-report-notes" className="space-y-1">
          {report.notes.map((n) => (
            <li key={n} className="flex items-start gap-1.5 text-[11px] text-amber-300">
              <AlertTriangle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
              <span>{n}</span>
            </li>
          ))}
        </ul>
      )}

      <div className="space-y-2">
        {report.operations.map((op) => (
          <OperationBlock key={op.id} op={op} />
        ))}
      </div>
    </section>
  );
};

