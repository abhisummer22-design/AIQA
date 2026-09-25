/**
 * Phase 7 — what an autopilot run would cost, as `autopilot_estimate.rs` works it out: calls, tokens, time and dollars as ranges,
 * for each step and for each task with the provider it would go to right now, and every assumption written down. Shown before a
 * run is made and again beside the button that accepts it.
 */
import React from 'react';
import { AlertTriangle, Coins, Info } from 'lucide-react';
import type { AutopilotEstimate } from '../../types/contract';
import {
  STEP_LABEL,
  formatCost,
  formatDurationRange,
  formatRange,
  formatTokens,
  latencyLabel,
  plural,
} from './autopilotModel';

const Tile: React.FC<{ id: string; label: string; value: string; note?: string; tone?: 'plain' | 'amber' }> = ({ id, label, value, note, tone = 'plain' }) => (
  <div id={id} className={`p-3 rounded-lg border ${tone === 'amber' ? 'bg-amber-950/30 border-amber-800/60' : 'bg-[#020617] border-slate-800'}`}>
    <div className="text-[10px] uppercase tracking-wide text-slate-500">{label}</div>
    <div className={`text-sm font-mono ${tone === 'amber' ? 'text-amber-200' : 'text-slate-100'}`}>{value}</div>
    {note && <div className="text-[10px] text-slate-500 mt-0.5">{note}</div>}
  </div>
);

export const EstimatePanel: React.FC<{ estimate: AutopilotEstimate }> = ({ estimate }) => {
  const { totals } = estimate;
  return (
    <section id="autopilot-estimate" className="space-y-3">
      <div className="grid grid-cols-2 md:grid-cols-4 gap-2">
        <Tile
          id="estimate-cases"
          label="Test cases"
          value={formatRange(estimate.cases)}
          note={`from ${estimate.requirements} ${plural(estimate.requirements, 'requirement', 'requirements')}`}
        />
        <Tile id="estimate-calls" label="AI calls" value={formatRange(totals.ai_calls)} />
        <Tile id="estimate-time" label="Time" value={formatDurationRange(totals.seconds)} note="not counting the wait for your approval" />
        <Tile
          id="estimate-cost"
          label="Cost"
          value={formatCost(totals.cost_usd)}
          note={totals.paid ? 'a provider that charges is used' : undefined}
          tone={totals.paid ? 'amber' : 'plain'}
        />
      </div>

      {estimate.warnings.length > 0 && (
        <ul id="estimate-warnings" className="space-y-1">
          {estimate.warnings.map((w) => (
            <li key={w} className="flex items-start gap-1.5 text-[11px] text-amber-300">
              <AlertTriangle className="w-3.5 h-3.5 mt-0.5 shrink-0" />
              <span>{w}</span>
            </li>
          ))}
        </ul>
      )}

      <div className="overflow-x-auto">
        <table id="estimate-steps" className="w-full text-[11px] text-left">
          <thead className="text-slate-500">
            <tr>
              <th className="py-1 pr-3 font-normal">Step</th>
              <th className="py-1 pr-3 font-normal">AI calls</th>
              <th className="py-1 pr-3 font-normal">Time</th>
              <th className="py-1 font-normal" />
            </tr>
          </thead>
          <tbody>
            {estimate.steps.map((s) => (
              <tr key={s.step} className="border-t border-slate-800/70">
                <td className="py-1 pr-3 text-slate-200">{STEP_LABEL[s.step]?.title ?? s.step}</td>
                <td className="py-1 pr-3 font-mono text-slate-300">{s.ai_calls.max === 0 ? '—' : formatRange(s.ai_calls)}</td>
                <td className="py-1 pr-3 font-mono text-slate-300">{s.seconds.max === 0 ? '—' : formatDurationRange(s.seconds)}</td>
                <td className="py-1 text-slate-500">{s.note ?? ''}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {estimate.tasks.length > 0 && (
        <div id="estimate-tasks" className="space-y-1.5">
          <h3 className="text-[11px] font-semibold text-slate-300 flex items-center gap-1.5">
            <Coins className="w-3.5 h-3.5 text-violet-400" /> Where the AI calls would go
          </h3>
          {estimate.tasks.map((t) => (
            <div key={t.task} id={`estimate-task-${t.task}`} className="p-2 rounded bg-[#020617] border border-slate-800 space-y-0.5">
              <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[11px]">
                <span className="text-slate-200">{t.used_for}</span>
                {t.provider ? (
                  <span className="font-mono text-violet-300">
                    {t.provider.name}/{t.provider.model}
                    <span className="text-slate-500"> · {t.provider.kind.toLowerCase()} · {t.provider.pricing.kind === 'UNMETERED' ? 'free' : 'charges'}</span>
                  </span>
                ) : (
                  <span className="font-mono text-rose-300">no provider can serve this</span>
                )}
              </div>
              <div className="text-[10px] text-slate-400 font-mono">
                {formatRange(t.calls)} {plural(t.calls.max, 'call', 'calls')} · {formatTokens(t.prompt_tokens.max)} tokens in, {formatTokens(t.completion_tokens.max)} out at most ·{' '}
                {formatDurationRange(t.seconds)} · {formatCost(t.cost_usd)}
              </div>
              <div className="text-[10px] text-slate-500">{latencyLabel(t.latency)}</div>
            </div>
          ))}
        </div>
      )}

      <details id="estimate-assumptions" className="text-[11px] text-slate-400">
        <summary className="cursor-pointer inline-flex items-center gap-1.5 text-slate-300">
          <Info className="w-3.5 h-3.5" /> What these figures rest on
        </summary>
        <ul className="mt-1.5 space-y-1 list-disc pl-5">
          {estimate.assumptions.map((a) => (
            <li key={a}>{a}</li>
          ))}
        </ul>
      </details>
    </section>
  );
};
