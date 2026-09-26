/**
 * Phase 8A — what the AI calls came to, as `ai_usage.rs` counted it: tiles for the whole period, a bar for each day, and the same
 * split by provider and model, by task and (for every project at once) by project.
 *
 * A cost is only ever counted for a call that answered, went to a provider that has a price and reported both its token counts.
 * The page says so wherever a cost is shown: a "—" is a row with no cost counted, and the tiles and the notes say how many calls
 * were left out and why, so a total never looks more certain than it is.
 */
import React, { useState } from 'react';
import { Link } from 'react-router-dom';
import { AlertTriangle, Info } from 'lucide-react';
import type { AiUsageReport, AiUsageTally } from '../../types/contract';
import { providerLabel } from '../autopilot/autopilotModel';
import {
  BAR_METRICS,
  type BarMetric,
  barFractions,
  costBasis,
  exactCount,
  formatCount,
  formatDay,
  formatMoney,
  formatMs,
  metricValue,
  taskLabel,
} from './aiUsageModel';

const Tile: React.FC<{ id: string; label: string; value: string; title?: string; note?: string; tone?: 'plain' | 'amber' }> = ({
  id,
  label,
  value,
  title,
  note,
  tone = 'plain',
}) => (
  <div id={id} className={`p-3 rounded-lg border ${tone === 'amber' ? 'bg-amber-950/30 border-amber-800/60' : 'bg-[#020617] border-slate-800'}`}>
    <div className="text-[10px] uppercase tracking-wide text-slate-500">{label}</div>
    <div className={`text-sm font-mono ${tone === 'amber' ? 'text-amber-200' : 'text-slate-100'}`} title={title}>
      {value}
    </div>
    {note && <div className="text-[10px] text-slate-500 mt-0.5">{note}</div>}
  </div>
);

/** The cost of a row: nothing is counted for a row with no priced call (`—`), and a row with some calls left out says so (`*`). */
export function costCell(tally: AiUsageTally): { text: string; title: string } {
  const basis = costBasis(tally);
  if (tally.priced_calls === 0) return { text: '—', title: `No cost counted: ${basis}` };
  const partial = tally.no_price_calls + tally.no_tokens_calls > 0;
  return { text: `${formatMoney(tally.cost_usd)}${partial ? '*' : ''}`, title: basis };
}

interface TableRow {
  id: string;
  name: React.ReactNode;
  tally: AiUsageTally;
}

const TallyTable: React.FC<{ id: string; title: string; nameHeading: string; rows: TableRow[] }> = ({ id, title, nameHeading, rows }) => (
  <section id={id} className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 space-y-2">
    <h3 className="text-[11px] font-semibold text-slate-300">{title}</h3>
    <div className="overflow-x-auto">
      <table className="w-full text-[11px] text-left">
        <caption className="sr-only">{title}</caption>
        <thead className="text-slate-500">
          <tr>
            <th className="py-1 pr-3 font-normal">{nameHeading}</th>
            <th className="py-1 pr-3 font-normal text-right">Calls</th>
            <th className="py-1 pr-3 font-normal text-right">Failed</th>
            <th className="py-1 pr-3 font-normal text-right">Tokens in</th>
            <th className="py-1 pr-3 font-normal text-right">Tokens out</th>
            <th className="py-1 pr-3 font-normal text-right">Avg answer</th>
            <th className="py-1 font-normal text-right">Cost</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(({ id: rowId, name, tally }) => {
            const cost = costCell(tally);
            return (
              <tr key={rowId} data-row={rowId} className="border-t border-slate-800/70">
                <td className="py-1 pr-3 text-slate-200">{name}</td>
                <td className="py-1 pr-3 font-mono text-slate-300 text-right">{exactCount(tally.calls)}</td>
                <td className={`py-1 pr-3 font-mono text-right ${tally.failed > 0 ? 'text-amber-300' : 'text-slate-500'}`}>{tally.failed}</td>
                <td className="py-1 pr-3 font-mono text-slate-300 text-right" title={exactCount(tally.prompt_tokens)}>
                  {formatCount(tally.prompt_tokens)}
                </td>
                <td className="py-1 pr-3 font-mono text-slate-300 text-right" title={exactCount(tally.completion_tokens)}>
                  {formatCount(tally.completion_tokens)}
                </td>
                <td className="py-1 pr-3 font-mono text-slate-300 text-right">{tally.avg_answer_ms === null ? '—' : formatMs(tally.avg_answer_ms)}</td>
                <td className="py-1 font-mono text-slate-200 text-right" title={cost.title}>
                  {cost.text}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  </section>
);

const DailyBars: React.FC<{ days: AiUsageReport['by_day'] }> = ({ days }) => {
  const [metric, setMetric] = useState<BarMetric>('calls');
  const fractions = barFractions(days, metric);
  const first = days[0];
  const last = days[days.length - 1];
  return (
    <section id="usage-days" className="p-3 rounded-lg bg-[#0B1120] border border-slate-800 space-y-2">
      <div className="flex items-center justify-between gap-2">
        <h3 className="text-[11px] font-semibold text-slate-300">By day</h3>
        <div role="group" aria-label="What the bars show" className="inline-flex rounded-md border border-slate-700 overflow-hidden">
          {BAR_METRICS.map((m) => (
            <button
              key={m.id}
              type="button"
              id={`usage-bars-${m.id}`}
              aria-pressed={metric === m.id}
              onClick={() => setMetric(m.id)}
              className={`px-2 py-0.5 text-[10px] cursor-pointer ${metric === m.id ? 'bg-violet-950/70 text-violet-200' : 'text-slate-400 hover:bg-slate-800'}`}
            >
              {m.label}
            </button>
          ))}
        </div>
      </div>
      <ol aria-label="Usage by day" className="flex items-end gap-px h-24">
        {days.map((d, i) => {
          const fraction = fractions[i];
          const tokens = d.prompt_tokens + d.completion_tokens;
          return (
            <li
              key={d.day}
              data-day={d.day}
              data-value={metricValue(d, metric)}
              title={`${formatDay(d.day)}: ${exactCount(d.calls)} ${d.calls === 1 ? 'call' : 'calls'}, ${formatCount(tokens)} tokens, ${formatMoney(d.cost_usd)}`}
              className="flex-1 h-full flex items-end min-w-0"
            >
              <span className="block w-full rounded-t bg-violet-500/80" style={{ height: fraction > 0 ? `${Math.max(fraction * 100, 3)}%` : '0%' }} />
            </li>
          );
        })}
      </ol>
      {first && last && (
        <div id="usage-days-axis" className="flex justify-between text-[10px] text-slate-500 font-mono">
          <span>{formatDay(first.day)}</span>
          <span>{formatDay(last.day)}</span>
        </div>
      )}
    </section>
  );
};

export const UsageReportView: React.FC<{
  report: AiUsageReport;
  providerName: (id: string) => string;
  projectName: (id: string | null) => string;
}> = ({ report, providerName, projectName }) => {
  const { totals } = report;
  const tokens = totals.prompt_tokens + totals.completion_tokens;
  return (
    <div id="ai-usage-report" className="space-y-4">
      <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
        <Tile
          id="usage-calls"
          label="AI calls"
          value={exactCount(totals.calls)}
          note={totals.failed > 0 ? `${totals.answered} answered · ${totals.failed} failed` : `${totals.answered} answered`}
          tone={totals.failed > 0 ? 'amber' : 'plain'}
        />
        <Tile
          id="usage-tokens"
          label="Tokens"
          value={`${formatCount(totals.prompt_tokens)} in · ${formatCount(totals.completion_tokens)} out`}
          title={`${exactCount(totals.prompt_tokens)} in, ${exactCount(totals.completion_tokens)} out (${exactCount(tokens)} together)`}
          note={totals.token_calls < totals.answered ? `reported by ${totals.token_calls} of ${totals.answered} answered calls` : undefined}
        />
        <Tile id="usage-cost" label="Estimated cost" value={formatMoney(totals.cost_usd)} note={costBasis(totals)} />
        <Tile
          id="usage-time"
          label="AI time"
          value={formatMs(totals.answered_ms)}
          note={
            [
              totals.avg_answer_ms !== null ? `${formatMs(totals.avg_answer_ms)} a call on average` : null,
              totals.failed_ms > 0 ? `${formatMs(totals.failed_ms)} spent on failed calls` : null,
            ]
              .filter(Boolean)
              .join(' · ') || undefined
          }
        />
      </div>

      <ul id="usage-notes" className="space-y-1">
        {report.notes.map((n) => (
          <li key={n} className="flex items-start gap-1.5 text-[11px] text-amber-300">
            <AlertTriangle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
            <span>{n}</span>
          </li>
        ))}
        <li className="flex items-start gap-1.5 text-[11px] text-slate-400">
          <Info className="w-3.5 h-3.5 mt-0.5 shrink-0" />
          <span>
            A price is set per provider, in{' '}
            <Link to="/settings" className="text-violet-300 underline">
              Settings
            </Link>
            ; a local model has none. Only calls that answered, to a provider with a price, are given a cost.
          </span>
        </li>
      </ul>

      <DailyBars days={report.by_day} />

      <TallyTable
        id="usage-by-provider"
        title="By provider and model"
        nameHeading="Provider / model"
        rows={report.by_provider.map((p) => ({ id: `${p.provider}/${p.model}`, name: <span className="font-mono">{providerLabel(`${p.provider}/${p.model}`, providerName)}</span>, tally: p }))}
      />
      <TallyTable
        id="usage-by-task"
        title="By task"
        nameHeading="Task"
        rows={report.by_task.map((t) => ({ id: t.task, name: taskLabel(t.task), tally: t }))}
      />
      {report.by_project && (
        <TallyTable
          id="usage-by-project"
          title="By project"
          nameHeading="Project"
          rows={report.by_project.map((p) => ({ id: p.project_id ?? 'none', name: projectName(p.project_id), tally: p }))}
        />
      )}
      <p id="usage-legend" className="text-[10px] text-slate-500">
        — no cost counted for the row (no price set, or no token counts) · * some calls of the row are left out of its cost
      </p>
    </div>
  );
};
